//! Real `Cortex` subprocesses over an isolated session store.
//!
//! Nothing here contacts a model or account service: every assertion is either a
//! local read of the session store or a validation failure produced before any
//! network use.

use cortex_engine::rollout::local::{SessionMeta, SessionStorage, StoredMessage};
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn cli(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_Cortex"))
        .current_dir(home)
        .env("CORTEX_HOME", home)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .env_remove("CORTEX_API_KEY")
        .env_remove("CORTEX_AUTH_TOKEN")
        .env_remove("CORTEX_LIVE_API")
        .stdin(Stdio::null())
        .args(args)
        .output()
        .unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn seed(home: &Path) -> (SessionStorage, SessionMeta) {
    let store = SessionStorage::with_dir(home.join("sessions"));
    let mut meta = SessionMeta::new("cortex", "test-model");
    meta.cwd = home.display().to_string();
    meta.title = Some("Recorded session".into());
    store
        .create_session(
            &meta,
            &[
                StoredMessage::user("first recorded prompt"),
                StoredMessage::assistant("recorded answer"),
                StoredMessage::user("second recorded prompt"),
            ],
        )
        .unwrap();
    (store, meta)
}

#[test]
fn session_inventory_and_history_render_the_stored_conversation() {
    let home = tempfile::tempdir().unwrap();
    let (_store, meta) = seed(home.path());

    let listed = cli(home.path(), &["sessions"]);
    assert!(listed.status.success(), "{}", text(&listed));
    let listed = text(&listed);
    assert!(listed.contains(meta.short_id()), "{listed}");
    assert!(listed.contains("Recorded session"), "{listed}");
    assert!(listed.contains("Total: 1 session(s)"), "{listed}");

    let history = cli(home.path(), &["history"]);
    assert!(history.status.success(), "{}", text(&history));
    let history = text(&history);
    assert!(history.contains("first recorded prompt"), "{history}");
    assert!(history.contains("second recorded prompt"), "{history}");
    // Assistant turns are not prompts and must not appear in prompt history.
    assert!(!history.contains("recorded answer"), "{history}");

    // A search narrows the same inventory rather than inventing new entries.
    let searched = cli(home.path(), &["history", "search", "second"]);
    assert!(searched.status.success(), "{}", text(&searched));
    assert!(text(&searched).contains("second recorded prompt"));
    assert!(!text(&searched).contains("first recorded prompt"));

    let empty = cli(home.path(), &["history", "search", "absent-phrase"]);
    assert!(empty.status.success(), "{}", text(&empty));
    assert!(!text(&empty).contains("recorded prompt"));
}

#[test]
fn history_and_sessions_limits_are_honored_in_json_and_text() {
    let home = tempfile::tempdir().unwrap();
    seed(home.path());
    let limited = cli(home.path(), &["history", "-n", "1", "--json"]);
    assert!(limited.status.success(), "{}", text(&limited));
    let prompts: Vec<serde_json::Value> = serde_json::from_slice(&limited.stdout).unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0]["prompt"], "second recorded prompt");

    let text_limited = cli(home.path(), &["history", "-n", "1"]);
    assert!(text_limited.status.success());
    assert!(text(&text_limited).contains("second recorded prompt"));
    assert!(!text(&text_limited).contains("first recorded prompt"));
}

#[test]
fn resume_refuses_impossible_or_unknown_sessions_without_touching_the_store() {
    let home = tempfile::tempdir().unwrap();
    let (store, meta) = seed(home.path());
    let stored = std::fs::read(store.history_path(&meta.id)).unwrap();

    // --no-session contradicts resuming a recorded session.
    let contradiction = cli(home.path(), &["resume", &meta.id, "--no-session"]);
    assert!(!contradiction.status.success());
    assert!(text(&contradiction).contains("--no-session"));

    // An unknown identifier is an error, not a silent fallback to the latest.
    let unknown = cli(home.path(), &["resume", "0123456789abcdef"]);
    assert!(!unknown.status.success());

    // "last" with an empty inventory has nothing to resume.
    let elsewhere = tempfile::tempdir().unwrap();
    let none = cli(elsewhere.path(), &["resume", "last"]);
    assert!(!none.status.success());
    assert!(text(&none).contains("No sessions found"));

    // Without a terminal the picker must refuse instead of choosing for us.
    let picker = cli(home.path(), &["resume", "--pick"]);
    assert!(!picker.status.success());
    assert!(text(&picker).contains("--last"), "{}", text(&picker));

    // --last selects the recorded session and validates its history; it then
    // stops at the terminal requirement rather than resuming blindly.
    for args in [
        vec!["resume", "--last", "--all"],
        vec!["resume", "last", "--all"],
    ] {
        let last = cli(home.path(), &args);
        assert!(!last.status.success(), "{args:?}");
        let reported = text(&last);
        assert!(
            !reported.contains("No sessions found"),
            "the recorded session must be selected: {reported}"
        );
    }

    assert_eq!(std::fs::read(store.history_path(&meta.id)).unwrap(), stored);
}

#[test]
fn deletion_requires_an_explicit_answer_when_there_is_no_terminal() {
    let home = tempfile::tempdir().unwrap();
    let (store, meta) = seed(home.path());
    let unconfirmed = cli(home.path(), &["delete", &meta.id]);
    assert!(!unconfirmed.status.success());
    assert!(
        text(&unconfirmed).contains("--yes"),
        "{}",
        text(&unconfirmed)
    );
    assert!(store.exists(&meta.id));
}

#[test]
fn interactive_startup_rejects_unusable_inputs_before_any_session_work() {
    let home = tempfile::tempdir().unwrap();
    let file = home.path().join("not-a-directory");
    std::fs::write(&file, "fixture").unwrap();

    let image = cli(home.path(), &["--image", "screenshot.png"]);
    assert!(!image.status.success());
    assert!(
        text(&image).contains("No image was sent"),
        "{}",
        text(&image)
    );

    let not_a_dir = cli(home.path(), &["--add-dir", file.to_str().unwrap()]);
    assert!(!not_a_dir.status.success());
    assert!(text(&not_a_dir).contains("not a directory"));

    let blank_model = cli(home.path(), &["--model", "   "]);
    assert!(!blank_model.status.success());
    assert!(text(&blank_model).contains("Model name cannot be empty"));

    // Rejected startups record no conversation.
    assert!(
        SessionStorage::with_dir(home.path().join("sessions"))
            .list_sessions()
            .unwrap()
            .is_empty()
    );
}
