//! Shared session and report state for the verification MCP.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use cortex_tui::actions::ActionMapper;
use cortex_tui::app::AppState;
use cortex_tui::runner::EventLoop;
use cortex_tui::session::{CortexSession, SessionStorage};

/// One headless TUI session. Built via `EventLoop::new`; `AppState` is stored
/// because `EventLoop` is not `Send` across the stdio MCP runtime.
pub struct TuiSession {
    pub app_state: AppState,
    pub mapper: ActionMapper,
    pub width: u16,
    pub height: u16,
    _home: tempfile::TempDir,
}

impl TuiSession {
    pub fn start(width: u16, height: u16, agent: bool) -> anyhow::Result<Self> {
        let home = tempfile::tempdir()?;
        let store = SessionStorage::with_dir(home.path().join("sessions"));
        let session = CortexSession::with_storage("cortex", "verify", store)?;
        let mut app = AppState::new();
        app.terminal_size = (width, height);
        app.agent_entrypoint = agent;
        let event_loop = EventLoop::new(app).with_cortex_session(session);
        Ok(Self {
            app_state: event_loop.app_state,
            mapper: ActionMapper::default(),
            width,
            height,
            _home: home,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateRow {
    pub id: String,
    pub pack: String,
    pub size: [u16; 2],
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_sha256: Option<String>,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowRow {
    pub id: String,
    pub status: String,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaletteSummary {
    pub accent: String,
    pub violet_px: u32,
    pub wash_px: u32,
    pub gold_px: u32,
    pub mint_px: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub schema: String,
    pub cli_version: String,
    pub sha: String,
    pub api_url: String,
    pub live: bool,
    pub sizes: Vec<[u16; 2]>,
    pub states: Vec<StateRow>,
    pub flows: Vec<FlowRow>,
    pub palette: PaletteSummary,
    pub summary: Summary,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    pub pass: u32,
    pub fail: u32,
    pub skip: u32,
    pub total: u32,
}

impl Default for VerifyReport {
    fn default() -> Self {
        Self {
            schema: "cortex-verify/1".into(),
            cli_version: env!("CARGO_PKG_VERSION").into(),
            sha: git_sha(),
            api_url: std::env::var("CORTEX_API_URL")
                .unwrap_or_else(|_| "https://api.cortex.foundation".into()),
            live: std::env::var("CORTEX_LIVE_API").ok().as_deref() == Some("1"),
            sizes: vec![[40, 12], [120, 40]],
            states: Vec::new(),
            flows: Vec::new(),
            palette: PaletteSummary {
                accent: "#1F4945".into(),
                violet_px: 0,
                wash_px: 0,
                gold_px: 0,
                mint_px: 0,
            },
            summary: Summary::default(),
            exit_code: 0,
        }
    }
}

pub struct VerifyState {
    pub sessions: HashMap<String, TuiSession>,
    pub report: VerifyReport,
    pub last_report: Option<Value>,
    pub last_report_path: Option<PathBuf>,
}

impl VerifyState {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            report: VerifyReport::default(),
            last_report: None,
            last_report_path: None,
        }
    }

    pub fn record_state(&mut self, row: StateRow) {
        if row.status == "fail" {
            self.report.summary.fail += 1;
        } else if row.status == "skip" {
            self.report.summary.skip += 1;
        } else {
            self.report.summary.pass += 1;
        }
        self.report.summary.total += 1;
        self.report.states.push(row);
    }

    pub fn record_flow(&mut self, row: FlowRow) {
        if row.status == "fail" {
            self.report.summary.fail += 1;
        } else if row.status == "skip" {
            self.report.summary.skip += 1;
        } else {
            self.report.summary.pass += 1;
        }
        self.report.summary.total += 1;
        self.report.flows.push(row);
    }
}

pub fn git_sha() -> String {
    if let Ok(hash) = std::env::var("CORTEX_GIT_HASH")
        && !hash.is_empty()
        && hash != "unknown"
    {
        return hash.chars().take(7).collect();
    }
    std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_start_and_report_counters() {
        let session = TuiSession::start(40, 12, false).expect("session");
        assert_eq!(session.width, 40);
        let agent = TuiSession::start(80, 24, true).expect("agent");
        assert!(agent.app_state.agent_entrypoint);

        let mut state = VerifyState::new();
        assert_eq!(state.report.schema, "cortex-verify/1");
        state.record_state(StateRow {
            id: "ok".into(),
            pack: "v2".into(),
            size: [40, 12],
            status: "pass".into(),
            frame_sha256: None,
            checks: vec![],
        });
        state.record_state(StateRow {
            id: "bad".into(),
            pack: "v2".into(),
            size: [40, 12],
            status: "fail".into(),
            frame_sha256: None,
            checks: vec![],
        });
        state.record_state(StateRow {
            id: "skip".into(),
            pack: "v2".into(),
            size: [40, 12],
            status: "skip".into(),
            frame_sha256: None,
            checks: vec![],
        });
        state.record_flow(FlowRow {
            id: "flow-ok".into(),
            status: "pass".into(),
            checks: vec![],
        });
        state.record_flow(FlowRow {
            id: "flow-bad".into(),
            status: "fail".into(),
            checks: vec![],
        });
        state.record_flow(FlowRow {
            id: "flow-skip".into(),
            status: "skip".into(),
            checks: vec![],
        });
        assert_eq!(state.report.summary.pass, 2);
        assert_eq!(state.report.summary.fail, 2);
        assert_eq!(state.report.summary.skip, 2);
        assert_eq!(state.report.summary.total, 6);
    }

    #[test]
    #[serial_test::serial]
    fn git_sha_prefers_env_and_falls_back() {
        let previous = std::env::var("CORTEX_GIT_HASH").ok();
        unsafe { std::env::set_var("CORTEX_GIT_HASH", "abcdef1234") };
        assert_eq!(git_sha(), "abcdef1");
        unsafe { std::env::set_var("CORTEX_GIT_HASH", "unknown") };
        let fallback = git_sha();
        assert!(!fallback.is_empty());
        unsafe { std::env::set_var("CORTEX_GIT_HASH", "") };
        assert!(!git_sha().is_empty());
        match previous {
            Some(hash) => unsafe { std::env::set_var("CORTEX_GIT_HASH", hash) },
            None => unsafe { std::env::remove_var("CORTEX_GIT_HASH") },
        }
    }
}
