//! Real CLI subprocesses, isolated data, no credentials or model service.
use cortex_engine::rollout::local::{SessionMeta, SessionStorage, StoredMessage, StoredToolCall};
use std::process::{Command, Output, Stdio};

fn cli(home: &std::path::Path, args: &[&str]) -> Output {
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

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn ux_contract_cli_inventory_export_import_history_protection() {
    let home = tempfile::tempdir().unwrap();
    let store = SessionStorage::with_dir(home.path().join("sessions"));
    let mut meta = SessionMeta::new("cortex", "test-model");
    meta.cwd = home.path().display().to_string();
    meta.title = Some("界🙂 canonical".into());
    meta.favorite = true;
    meta.updated_at = chrono::DateTime::parse_from_rfc3339("2024-02-20T12:00:00Z")
        .unwrap()
        .into();
    let messages = vec![
        StoredMessage::system("keep system role"),
        StoredMessage::user("recover café"),
        StoredMessage::assistant("").with_tool_call(StoredToolCall::new(
            "tool-1",
            "Read",
            serde_json::json!({"path":"file.rs"}),
        )),
        StoredMessage::tool_result("tool-1", "file content"),
    ];
    store.create_session(&meta, &messages).unwrap();
    let original = std::fs::read(store.history_path(&meta.id)).unwrap();
    let list = success(cli(
        home.path(),
        &[
            "sessions",
            "--all",
            "--favorites",
            "--since",
            "2024-02-01",
            "--until",
            "2024-02-29",
            "--search",
            "CAFÉ",
            "--json",
        ],
    ));
    let listed: Vec<serde_json::Value> = serde_json::from_str(&list).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], meta.id);

    let exported = success(cli(home.path(), &["export", &meta.id]));
    let document: cortex_engine::rollout::local::SessionDocument =
        serde_json::from_str(&exported).unwrap();
    assert_eq!(document.meta.id, meta.id);
    let path = home.path().join("export.json");
    std::fs::write(&path, exported).unwrap();
    success(cli(home.path(), &["import", path.to_str().unwrap()]));
    let sessions = store.list_sessions().unwrap();
    assert_eq!(sessions.len(), 2);
    let new_id = sessions
        .iter()
        .find(|s| s.id != meta.id)
        .unwrap()
        .id
        .clone();
    assert_eq!(
        serde_json::to_value(store.load_messages(&new_id).unwrap()).unwrap(),
        serde_json::to_value(messages).unwrap()
    );
    assert_eq!(
        std::fs::read(store.history_path(&meta.id)).unwrap(),
        original
    );
    let history = success(cli(
        home.path(),
        &["history", "--all", "search", "café", "--json", "-n", "1"],
    ));
    let prompts: Vec<serde_json::Value> = serde_json::from_str(&history).unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0]["prompt"], "recover café");

    success(cli(home.path(), &["lock", "add", meta.short_id()]));
    assert!(
        !cli(home.path(), &["delete", &meta.id, "--yes"])
            .status
            .success()
    );
    assert_eq!(
        std::fs::read(store.history_path(&meta.id)).unwrap(),
        original
    );
    assert!(
        !cli(home.path(), &["resume", "--pick", "--all"])
            .status
            .success()
    );
    assert!(
        !cli(home.path(), &["sessions", "--since", "invalid"])
            .status
            .success()
    );
    assert!(
        !cli(home.path(), &["history", "clear", "--yes"])
            .status
            .success()
    );
    success(cli(home.path(), &["lock", "remove", &meta.id, "--yes"]));
    success(cli(home.path(), &["delete", &meta.id, "--yes"]));
    assert!(!store.exists(&meta.id));
    assert!(store.exists(&new_id));
}

#[test]
fn ux_contract_cli_rejects_bad_import_and_existing_export_without_mutation() {
    let home = tempfile::tempdir().unwrap();
    let store = SessionStorage::with_dir(home.path().join("sessions"));
    let meta = SessionMeta::new("cortex", "test");
    store.create_session(&meta, &[]).unwrap();
    let existing = home.path().join("important.json");
    std::fs::write(&existing, "user data").unwrap();
    assert!(
        !cli(
            home.path(),
            &["export", &meta.id, "--output", existing.to_str().unwrap()]
        )
        .status
        .success()
    );
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "user data");
    std::fs::write(&existing, format!("{}{{bad json", "界".repeat(75))).unwrap();
    assert!(
        !cli(home.path(), &["import", existing.to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(store.list_sessions().unwrap().len(), 1);
    assert!(
        !cli(home.path(), &["delete", "../escape", "--yes", "--force"])
            .status
            .success()
    );
}
