//! `/jobs` picker — background agents and subagents (lock `jobs`).

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
}
