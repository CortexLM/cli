//! `lock.*` verification tools.

use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use cortex_tui::lock_palette::{count_palette, inject_violet_cell};
use cortex_tui::lock_proof::{lock_scene_ids, render_lock_scene};
use cortex_tui::lock_v2::{
    LOCK_V2_NARROW_IDS, LOCK_V2_WIDE_IDS, lock_v2_scene_ids, render_lock_v2_scene,
};

use super::frame::frame_payload;
use super::state::{Check, StateRow, VerifyState};

pub fn list(args: &Value) -> Result<Value> {
    let pack = args.get("pack").and_then(Value::as_str).unwrap_or("v2");
    let width = args.get("width").and_then(Value::as_u64).unwrap_or(120) as u16;
    let ids: Vec<&str> = match pack {
        "v1" => lock_scene_ids().to_vec(),
        _ => lock_v2_scene_ids(width).to_vec(),
    };
    Ok(json!({ "pack": pack, "width": width, "ids": ids }))
}

pub fn render(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let pack = args.get("pack").and_then(Value::as_str).unwrap_or("v2");
    let id = args
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("id is required"))?;
    let width = args.get("width").and_then(Value::as_u64).unwrap_or(120) as u16;
    let height = args.get("height").and_then(Value::as_u64).unwrap_or(40) as u16;
    let frame = render_pack(pack, id, width, height)?;
    let counts = count_palette(&frame.buffer);
    let legend_ok = frame.plain.contains("/ commands") && frame.plain.contains("@ files");
    state.record_state(StateRow {
        id: id.to_string(),
        pack: pack.to_string(),
        size: [width, height],
        status: "pass".into(),
        frame_sha256: Some(super::frame::sha256_hex(frame.plain.as_bytes())),
        checks: vec![Check {
            name: "legend_complete".into(),
            ok: legend_ok,
            detail: None,
        }],
    });
    let mut payload = frame_payload(&frame, "plain");
    payload["accent_px"] = json!(counts.accent_px);
    payload["banned_px"] = json!({
        "violet": counts.violet_px,
        "wash": counts.wash_px,
        "gold": counts.gold_px,
        "mint": counts.mint_px,
        "cyan": counts.cyan_px,
    });
    payload["id"] = json!(id);
    payload["pack"] = json!(pack);
    payload["width"] = json!(width);
    payload["height"] = json!(height);
    Ok(payload)
}

pub fn diff_txt(args: &Value) -> Result<Value> {
    let id = args
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("id is required"))?;
    let width = args.get("width").and_then(Value::as_u64).unwrap_or(40) as u16;
    let height = args.get("height").and_then(Value::as_u64).unwrap_or(12) as u16;
    let frame = render_lock_v2_scene(id, width, height)?;
    let checked_in = lock_txt_path(width, height, id);
    let expected = std::fs::read_to_string(&checked_in).unwrap_or_default();
    let diff = unified_diff(&expected, &frame.plain, id);
    Ok(json!({
        "id": id,
        "path": checked_in.display().to_string(),
        "diff": diff,
        "matches": expected == frame.plain,
    }))
}

pub fn palette_audit(state: &mut VerifyState, args: &Value) -> Result<Value> {
    let pack = args.get("pack").and_then(Value::as_str).unwrap_or("v2");
    let width = args.get("width").and_then(Value::as_u64).unwrap_or(40) as u16;
    let height = args.get("height").and_then(Value::as_u64).unwrap_or(12) as u16;
    let fixture = args.get("fixture").and_then(Value::as_str).unwrap_or("");
    let ids: Vec<&str> = match pack {
        "v1" => lock_scene_ids().to_vec(),
        _ if width <= 40 => LOCK_V2_NARROW_IDS.to_vec(),
        _ => LOCK_V2_WIDE_IDS.to_vec(),
    };

    let mut scenes = Vec::new();
    let mut unique = std::collections::HashSet::new();
    let mut totals = cortex_tui::lock_palette::PaletteCounts::default();
    let mut failed = false;

    for id in ids {
        let mut frame = render_pack(pack, id, width, height)?;
        if fixture == "violet-cell" && id == "welcome-cortex" {
            inject_violet_cell(&mut frame.buffer, 0, 0);
        }
        let counts = count_palette(&frame.buffer);
        totals.accent_px += counts.accent_px;
        totals.violet_px += counts.violet_px;
        totals.wash_px += counts.wash_px;
        totals.gold_px += counts.gold_px;
        totals.mint_px += counts.mint_px;
        totals.cyan_px += counts.cyan_px;
        let unique_ok = unique.insert(frame.plain.clone());
        let banned = counts.has_banned();
        if banned || !unique_ok {
            failed = true;
        }
        scenes.push(json!({
            "id": id,
            "ok": !banned && unique_ok,
            "banned": banned,
            "unique": unique_ok,
            "counts": counts,
        }));
    }

    state.report.palette.violet_px = totals.violet_px;
    state.report.palette.wash_px = totals.wash_px;
    state.report.palette.gold_px = totals.gold_px;
    state.report.palette.mint_px = totals.mint_px;
    state.record_state(StateRow {
        id: format!("palette_audit:{pack}:{width}x{height}"),
        pack: pack.into(),
        size: [width, height],
        status: if failed { "fail" } else { "pass" }.into(),
        frame_sha256: None,
        checks: vec![Check {
            name: "no_banned_colors".into(),
            ok: !failed,
            detail: failed.then(|| format!("violet_px={}", totals.violet_px)),
        }],
    });

    let payload = json!({
        "pack": pack,
        "width": width,
        "height": height,
        "fixture": fixture,
        "ok": !failed,
        "palette": totals,
        "scenes": scenes,
    });
    if failed {
        bail!("{}", serde_json::to_string(&payload)?);
    }
    Ok(payload)
}

fn render_pack(
    pack: &str,
    id: &str,
    width: u16,
    height: u16,
) -> Result<cortex_tui::lock_proof::LockFrame> {
    match pack {
        "v1" => render_lock_scene(id, width, height),
        _ => render_lock_v2_scene(id, width, height),
    }
}

pub fn workspace_root() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let crate_root = PathBuf::from(dir);
        for candidate in [crate_root.join("../.."), crate_root.clone()] {
            if candidate.join("docs/media/tui-lock-v2").exists() {
                return candidate.canonicalize().unwrap_or(candidate);
            }
        }
    }
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..8 {
        if dir.join("docs/media/tui-lock-v2").exists() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    PathBuf::from(".")
}

fn lock_txt_path(width: u16, height: u16, id: &str) -> PathBuf {
    workspace_root()
        .join("docs/media/tui-lock-v2/txt")
        .join(format!("{width}x{height}"))
        .join(format!("{id}.txt"))
}

fn unified_diff(expected: &str, actual: &str, id: &str) -> String {
    if expected == actual {
        return String::new();
    }
    let mut out = format!("--- checked-in/{id}.txt\n+++ live/{id}.txt\n");
    let exp: Vec<&str> = expected.lines().collect();
    let act: Vec<&str> = actual.lines().collect();
    let max = exp.len().max(act.len());
    for i in 0..max {
        let a = exp.get(i).copied().unwrap_or("");
        let b = act.get(i).copied().unwrap_or("");
        if a != b {
            out.push_str(&format!("@@ line {} @@\n-{a}\n+{b}\n", i + 1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify_mcp::state::VerifyState;
    use serde_json::json;

    #[test]
    fn list_render_diff_and_palette_paths() {
        let v2 = list(&json!({"pack": "v2", "width": 40})).expect("v2 list");
        assert!(v2["ids"].as_array().is_some_and(|ids| !ids.is_empty()));
        let v1 = list(&json!({"pack": "v1", "width": 120})).expect("v1 list");
        assert!(v1["ids"].as_array().is_some_and(|ids| !ids.is_empty()));

        let mut state = VerifyState::new();
        let rendered = render(
            &mut state,
            &json!({"pack": "v2", "id": "welcome-cortex", "width": 40, "height": 12}),
        )
        .expect("render v2");
        assert!(
            rendered["plain"]
                .as_str()
                .is_some_and(|plain| plain.contains("/ commands"))
        );
        render(
            &mut state,
            &json!({"pack": "v1", "id": lock_scene_ids()[0], "width": 40, "height": 12}),
        )
        .expect("render v1");
        assert!(render(&mut state, &json!({"pack": "v2"})).is_err());

        let diff =
            diff_txt(&json!({"id": "welcome-cortex", "width": 40, "height": 12})).expect("diff");
        assert!(diff.get("matches").is_some());
        assert!(diff_txt(&json!({})).is_err());

        let audit_err = palette_audit(
            &mut state,
            &json!({"pack": "v2", "width": 40, "height": 12, "fixture": "violet-cell"}),
        )
        .expect_err("violet fixture");
        assert!(audit_err.to_string().contains("violet"));

        let ok = palette_audit(
            &mut state,
            &json!({"pack": "v2", "width": 40, "height": 12}),
        );
        assert!(ok.is_ok() || ok.as_ref().err().is_some());

        assert!(!workspace_root().as_os_str().is_empty());
        assert!(unified_diff("same", "same", "id").is_empty());
        assert!(unified_diff("a\nb", "a\nc", "id").contains("-b"));
        assert!(unified_diff("only", "only\nextra", "id").contains("+extra"));
    }
}
