//! Local session dispatch shared by inventory, resume and prompt history.
use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use cortex_engine::rollout::local::{SessionMeta, SessionStorage};
use std::io::{IsTerminal, Write};

pub(super) fn storage() -> Result<SessionStorage> {
    SessionStorage::new()
}

pub(super) fn pick_session(sessions: &[SessionMeta]) -> Result<String> {
    if sessions.is_empty() {
        bail!("No sessions found");
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        bail!(
            "A session ID is required without an interactive terminal; use --last explicitly to choose the latest"
        );
    }
    for (index, session) in sessions.iter().enumerate() {
        eprintln!(
            "{}. {}  {}",
            index + 1,
            session.short_id(),
            session.display_title()
        );
    }
    eprint!("Session number (empty to cancel): ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    let index = answer
        .trim()
        .parse::<usize>()
        .context("Session selection cancelled or invalid")?;
    sessions
        .get(index.checked_sub(1).context("Invalid session number")?)
        .map(|s| s.id.clone())
        .context("Invalid session number")
}

#[derive(Default)]
pub(super) struct Filters<'a> {
    pub all: bool,
    pub days: Option<u32>,
    pub since: Option<&'a str>,
    pub until: Option<&'a str>,
    pub favorites: bool,
    pub search: Option<&'a str>,
    pub limit: Option<usize>,
}

pub(super) fn filter_sessions(
    store: &SessionStorage,
    filters: Filters<'_>,
) -> Result<Vec<SessionMeta>> {
    let since = filters.since.map(|v| parse_date(v, false)).transpose()?;
    let until = filters.until.map(|v| parse_date(v, true)).transpose()?;
    if let (Some(from), Some(to)) = (since, until)
        && from > to
    {
        bail!("--since must not be later than --until");
    }
    let cutoff = filters
        .days
        .map(|days| Utc::now() - chrono::Duration::days(i64::from(days)));
    let cwd = std::env::current_dir()?;
    let summaries = match filters.search {
        Some(query) => store.search(query)?,
        None => store.list_sessions()?,
    };
    let mut result = Vec::new();
    for summary in summaries {
        let meta = store.load_meta(&summary.id)?;
        if !filters.all && std::path::Path::new(&meta.cwd) != cwd
            || filters.favorites && !meta.favorite
            || since.is_some_and(|date| meta.updated_at < date)
            || until.is_some_and(|date| meta.updated_at > date)
            || cutoff.is_some_and(|date| meta.updated_at < date)
        {
            continue;
        }
        result.push(meta);
    }
    result.truncate(filters.limit.unwrap_or(usize::MAX));
    Ok(result)
}

fn parse_date(value: &str, end: bool) -> Result<DateTime<Utc>> {
    if let Ok(date) = DateTime::parse_from_rfc3339(value) {
        return Ok(date.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .context("Expected YYYY-MM-DD or RFC3339 date")?;
    let time = if end {
        date.and_hms_nano_opt(23, 59, 59, 999_999_999)
    } else {
        date.and_hms_opt(0, 0, 0)
    };
    Ok(time.context("Invalid date")?.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_engine::rollout::local::StoredMessage;

    #[test]
    fn ux_contract_filters_title_content_favorites_dates_and_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStorage::with_dir(temp.path().to_path_buf());
        for (title, favorite, date, text) in [
            ("Alpha", true, "2024-02-20", "café recovery"),
            ("Beta", false, "2024-03-20", "unrelated"),
        ] {
            let mut meta = SessionMeta::new("cortex", "test");
            // Recorded against a directory that is never the test's own cwd, so
            // the default (cwd-scoped) filter must hide them.
            meta.cwd = temp.path().join("elsewhere").display().to_string();
            meta.title = Some(title.into());
            meta.favorite = favorite;
            meta.updated_at = parse_date(date, false).unwrap();
            store
                .create_session(&meta, &[StoredMessage::user(text)])
                .unwrap();
        }
        let filtered = filter_sessions(
            &store,
            Filters {
                all: true,
                favorites: true,
                since: Some("2024-02-01"),
                until: Some("2024-02-29"),
                search: Some("CAFÉ"),
                ..Filters::default()
            },
        )
        .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title.as_deref(), Some("Alpha"));
        assert!(parse_date("not-a-date", false).is_err());
        assert_eq!(
            parse_date("2024-02-20T12:00:00+02:00", false).unwrap(),
            parse_date("2024-02-20T10:00:00Z", true).unwrap()
        );
        assert!(parse_date("2024-02-20", true).unwrap() > parse_date("2024-02-20", false).unwrap());
        assert!(
            filter_sessions(
                &store,
                Filters {
                    since: Some("2025-01-01"),
                    until: Some("2024-01-01"),
                    ..Filters::default()
                }
            )
            .is_err()
        );
        let limited = filter_sessions(
            &store,
            Filters {
                all: true,
                limit: Some(1),
                ..Filters::default()
            },
        )
        .unwrap();
        assert_eq!(limited.len(), 1);
        // Sessions recorded for another directory stay hidden without --all.
        assert!(
            filter_sessions(&store, Filters::default())
                .unwrap()
                .is_empty()
        );
        // A cutoff older than every session hides them all.
        assert!(
            filter_sessions(
                &store,
                Filters {
                    all: true,
                    days: Some(1),
                    ..Filters::default()
                }
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn picking_refuses_an_empty_inventory_instead_of_prompting() {
        assert_eq!(
            pick_session(&[]).unwrap_err().to_string(),
            "No sessions found"
        );
        // The non-interactive refusal is asserted end-to-end by the CLI
        // subprocess test; the prompt itself needs a real TTY and is not
        // exercised here so the suite can never block on stdin.
    }
}
