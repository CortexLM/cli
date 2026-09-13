//! Resume picker — `/resume` session list (lock `resume-picker`).

use crate::interactive::state::{InteractiveAction, InteractiveItem, InteractiveState};
use crate::session::SessionSummary;
use chrono::{Duration, Utc};

/// Build the `/resume` list: search chrome, recent sessions, no New Session row.
///
/// Shortcuts: Enter resume · `f` favorite · `d` delete · Esc close.
pub fn build_resume_picker(sessions: &[SessionSummary], show_archived: bool) -> InteractiveState {
    let filtered_sessions: Vec<_> = sessions
        .iter()
        .filter(|s| show_archived || !s.archived)
        .take(15)
        .collect();

    let items: Vec<InteractiveItem> = filtered_sessions
        .iter()
        .map(|session| {
            let title = if session.title.is_empty() {
                "Untitled session"
            } else {
                session.title.as_str()
            };
            let display_title = if title.chars().count() > 40 {
                format!("{}...", title.chars().take(37).collect::<String>())
            } else {
                title.to_string()
            };
            InteractiveItem::new(&session.id, display_title).with_description(resume_meta(session))
        })
        .collect();

    InteractiveState::new(
        "Resume Session".to_string(),
        items,
        InteractiveAction::ResumeSession,
    )
    .with_search()
    .with_max_visible(12)
    .with_hints(vec![
        ("Enter".to_string(), "resume".to_string()),
        ("f".to_string(), "favorite".to_string()),
        ("d".to_string(), "delete".to_string()),
        ("Esc".to_string(), "close".to_string()),
    ])
}

fn resume_meta(session: &SessionSummary) -> String {
    let time_ago = format_time_ago(session.updated_at);
    let extra = session.model.trim();
    if extra.contains('/') {
        format!(
            "{} · {} messages · {extra}",
            time_ago, session.message_count
        )
    } else {
        format!("{} · {} messages", time_ago, session.message_count)
    }
}

/// Format a timestamp as "X ago" human-readable string.
fn format_time_ago(timestamp: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(timestamp);

    if diff < Duration::minutes(1) {
        "just now".to_string()
    } else if diff < Duration::hours(1) {
        let mins = diff.num_minutes();
        format!("{}m ago", mins)
    } else if diff < Duration::hours(24) {
        let hours = diff.num_hours();
        format!("{}h ago", hours)
    } else if diff < Duration::days(7) {
        let days = diff.num_days();
        if days == 1 {
            "yesterday".to_string()
        } else {
            format!("{} days ago", days)
        }
    } else if diff < Duration::days(30) {
        let weeks = diff.num_weeks();
        format!("{}w ago", weeks)
    } else {
        timestamp.format("%b %d").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_session(id: &str, title: &str, hours_ago: i64) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            title: title.to_string(),
            message_count: 5,
            created_at: Utc::now() - Duration::hours(hours_ago),
            updated_at: Utc::now() - Duration::hours(hours_ago),
            archived: false,
            provider: "cortex".to_string(),
            model: "cortex-1-mini".to_string(),
        }
    }

    #[test]
    fn test_build_resume_picker_empty() {
        let state = build_resume_picker(&[], false);
        assert!(state.items.is_empty());
        assert!(state.searchable);
        assert!(state.inline_search());
    }

    #[test]
    fn test_build_resume_picker_with_sessions() {
        let sessions = vec![
            create_test_session("abc123", "Fix auth bug", 1),
            create_test_session("def456", "Add user profile", 24),
            create_test_session("ghi789", "Refactor database", 72),
        ];
        let state = build_resume_picker(&sessions, false);

        assert_eq!(state.items.len(), 3);
        assert_eq!(state.items[0].id, "abc123");
        assert!(!state.items.iter().any(|i| i.id == "__new__"));
        let lower = format!(
            "{}{}",
            state.items[0].label,
            state.items[0].description.as_deref().unwrap_or("")
        )
        .to_ascii_lowercase();
        assert!(!lower.contains("claude"));
        assert!(!lower.contains("anthropic"));
    }

    #[test]
    fn test_format_time_ago() {
        let now = Utc::now();

        assert_eq!(format_time_ago(now), "just now");
        assert_eq!(format_time_ago(now - Duration::minutes(30)), "30m ago");
        assert_eq!(format_time_ago(now - Duration::hours(2)), "2h ago");
        assert_eq!(format_time_ago(now - Duration::days(1)), "yesterday");
        assert_eq!(format_time_ago(now - Duration::days(3)), "3 days ago");
    }

    #[test]
    fn branch_slug_appends_when_model_looks_like_a_ref() {
        let mut session = create_test_session("s1", "fix login redirect", 2);
        session.model = "cortex/fix-login-redirect".into();
        session.message_count = 14;
        let meta = resume_meta(&session);
        assert!(meta.contains("14 messages · cortex/fix-login-redirect"));
    }
}
