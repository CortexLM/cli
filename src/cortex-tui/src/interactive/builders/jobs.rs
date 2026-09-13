//! `/jobs` picker — background agents and subagents (lock `jobs`).

use cortex_engine::client::CodeSession;

use crate::app::SubagentTaskDisplay;
use crate::interactive::state::{InteractiveAction, InteractiveItem, InteractiveState};

/// One row in the jobs picker.
#[derive(Debug, Clone)]
pub struct JobRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
}

impl JobRow {
    pub fn from_subagent(task: &SubagentTaskDisplay) -> Self {
        Self {
            id: task.session_id.clone(),
            kind: if task.agent_type.is_empty() {
                "subagent".into()
            } else {
                task.agent_type.clone()
            },
            title: task.description.clone(),
            status: task.status.description(),
        }
    }

    pub fn from_code_session(session: &CodeSession) -> Self {
        let title = if session.title.is_empty() {
            session.id.clone()
        } else {
            session.title.clone()
        };
        let kind = if session.runtime.is_empty() {
            "cloud".into()
        } else {
            session.runtime.clone()
        };
        let status = if session.state.is_empty() {
            "live".into()
        } else {
            session.state.clone()
        };
        Self {
            id: session.id.clone(),
            kind,
            title,
            status,
        }
    }
}

/// Build `/jobs` interactive picker. Numbered `>` / `·` rows — not radios.
pub fn build_jobs_picker(jobs: &[JobRow]) -> InteractiveState {
    let running = jobs.iter().filter(|j| job_is_running(&j.status)).count();
    let title = if jobs.is_empty() {
        "Agents & jobs · none running".into()
    } else {
        format!("Agents & jobs · {running} running")
    };

    let items = if jobs.is_empty() {
        vec![
            InteractiveItem::new("empty", "No background tasks running")
                .with_description("Ctrl+B runs the current prompt in the background")
                .with_disabled(true),
        ]
    } else {
        jobs.iter()
            .map(|job| {
                InteractiveItem::new(job.id.clone(), format!("{} · {}", job.kind, job.title))
                    .with_description(job.status.clone())
            })
            .collect()
    };

    InteractiveState::new(
        title,
        items,
        InteractiveAction::Custom("jobs-picker".into()),
    )
    .with_hints(vec![
        ("Enter".into(), "open".into()),
        ("a".into(), "attach".into()),
        ("x".into(), "cancel job".into()),
        ("Esc".into(), "close".into()),
    ])
}

pub(crate) fn job_is_running(status: &str) -> bool {
    let s = status.to_ascii_lowercase();
    !(s.contains("done")
        || s.contains("fail")
        || s.contains("cancel")
        || s.contains("wait")
        || s.contains("queue")
        || s.contains("stop"))
}

pub const JOBS_ATTACH_PREFIX: &str = "attach:";
pub const JOBS_STOP_PREFIX: &str = "stop:";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{SubagentDisplayStatus, SubagentTaskDisplay};

    #[test]
    fn empty_jobs_picker_is_honest() {
        let state = build_jobs_picker(&[]);
        assert!(state.title.contains("none running"));
        assert!(state.items[0].disabled);
        assert!(matches!(
            state.action,
            InteractiveAction::Custom(ref id) if id == "jobs-picker"
        ));
    }

    #[test]
    fn jobs_from_code_sessions_keep_api_ids() {
        let session = CodeSession {
            id: "sess-lock-1".into(),
            runtime: "cloud".into(),
            kind: String::new(),
            host_status: String::new(),
            state: String::new(),
            model_slug: String::new(),
            model_ref: String::new(),
            title: "rate limiter".into(),
            created_at: String::new(),
            host_id: String::new(),
            stream: None,
        };
        let row = JobRow::from_code_session(&session);
        assert_eq!(row.id, "sess-lock-1");
        assert_eq!(row.kind, "cloud");
        assert_eq!(row.title, "rate limiter");
        assert_eq!(row.status, "live");
    }

    #[test]
    fn jobs_from_subagents_keep_ids() {
        let mut task = SubagentTaskDisplay::new("sub-1", "tool-1", "rate-limiter", "subagent");
        task.status = SubagentDisplayStatus::ExecutingTool("edit".into());
        let rows = vec![JobRow::from_subagent(&task)];
        let state = build_jobs_picker(&rows);
        assert_eq!(state.items[0].id, "sub-1");
        assert!(state.title.contains("1 running"));
        assert!(state.hints.as_ref().unwrap().iter().any(|(k, _)| k == "a"));
    }

    #[test]
    fn queued_jobs_are_not_counted_as_running() {
        let rows = vec![JobRow {
            id: "q".into(),
            kind: "queued".into(),
            title: "later".into(),
            status: "waiting".into(),
        }];
        let state = build_jobs_picker(&rows);
        assert!(state.title.contains("0 running"));
        assert!(!job_is_running("waiting"));
        assert!(job_is_running("running"));
    }

    #[test]
    fn jobs_picker_uses_numbered_chevron_chrome_not_radios() {
        use crate::interactive::renderer::InteractiveWidget;
        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

        let rows = vec![
            JobRow {
                id: "cloud".into(),
                kind: "cloud agent".into(),
                title: "bc-4f2a".into(),
                status: "running".into(),
            },
            JobRow {
                id: "q".into(),
                kind: "queued".into(),
                title: "later".into(),
                status: "waiting".into(),
            },
        ];
        let state = build_jobs_picker(&rows);
        let area = Rect::new(0, 0, 72, 10);
        let mut buf = Buffer::empty(area);
        InteractiveWidget::new(&state).render(area, &mut buf);
        let mut text = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(
            text.lines()
                .any(|line| line.trim_start().starts_with('>') || line.starts_with("> ")),
            "selected row must use `>` chrome, got {text}"
        );
        assert!(
            text.contains("· "),
            "unselected rows must use middot chrome, got {text}"
        );
        assert!(
            !text.contains('●') && !text.contains('○'),
            "jobs picker must not use radio glyphs, got {text}"
        );
    }
}
