use super::*;

fn fixture() -> (tempfile::TempDir, SessionStorage, SessionMeta) {
    let home = tempfile::tempdir().unwrap();
    let store = SessionStorage::with_dir(home.path().join("sessions"));
    let meta = SessionMeta::new("cortex", "test-model");
    (home, store, meta)
}

#[test]
fn ux_contract_full_fidelity_roundtrip_and_distinct_identity() {
    let (_home, store, mut meta) = fixture();
    meta.favorite = true;
    meta.protected = true;
    meta.title = Some("界🙂e\u{301}".repeat(25));
    let messages = vec![
        StoredMessage::system("Retain instructions"),
        StoredMessage::user("Inspect source"),
        StoredMessage::assistant("")
            .with_tool_call(StoredToolCall::new(
                "call-1",
                "Read",
                serde_json::json!({"path":"file.rs"}),
            ))
            .with_reasoning("Inspect before editing")
            .with_tokens(10, 20),
        StoredMessage::tool_result("call-1", "source"),
    ];
    store.create_session(&meta, &messages).unwrap();
    let original = std::fs::read(store.history_path(&meta.id)).unwrap();
    let document = store.document(&meta.id).unwrap();
    let new_id = store
        .import_document(serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap())
        .unwrap();
    assert_ne!(new_id, meta.id);
    let imported = store.document(&new_id).unwrap();
    assert_eq!(
        serde_json::to_value(imported.messages).unwrap(),
        serde_json::to_value(messages).unwrap()
    );
    assert_eq!(imported.meta.cwd, meta.cwd);
    assert_eq!(imported.meta.model, meta.model);
    assert_eq!(imported.meta.title, meta.title);
    assert!(imported.meta.favorite);
    assert!(!imported.meta.protected);
    assert_eq!(
        std::fs::read(store.history_path(&meta.id)).unwrap(),
        original
    );
}

#[test]
fn ux_contract_legacy_read_and_promotion_preserve_original_bytes() {
    let (_home, store, meta) = fixture();
    store.ensure_base_dir().unwrap();
    let legacy = store.base_dir().join(format!("{}.jsonl", meta.id));
    let content = format!(
        "{{\"timestamp\":\"2024-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"timestamp\":\"2024-01-01T00:00:00Z\",\"cwd\":\"/tmp\",\"model\":\"test\"}}}}\n{{\"timestamp\":\"2024-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\",\"message\":\"legacy prompt\"}}}}\n",
        meta.id
    );
    std::fs::write(&legacy, &content).unwrap();
    assert_eq!(store.list_sessions().unwrap().len(), 1);
    assert_eq!(
        store.load_messages(&meta.id).unwrap()[0].content,
        "legacy prompt"
    );
    assert!(
        !store.session_dir(&meta.id).exists(),
        "reads must not migrate"
    );
    let mut loaded = store.load_meta(&meta.id).unwrap();
    loaded.favorite = true;
    store.save_meta(&loaded).unwrap();
    assert_eq!(store.load_messages(&meta.id).unwrap().len(), 1);
    assert_eq!(
        store.list_sessions().unwrap().len(),
        1,
        "deduplicate representations"
    );
    assert_eq!(std::fs::read_to_string(legacy).unwrap(), content);
}

#[test]
fn ux_contract_protection_corruption_and_recoverable_delete() {
    let (home, store, meta) = fixture();
    store
        .create_session(&meta, &[StoredMessage::user("keep me")])
        .unwrap();
    let history = std::fs::read(store.history_path(&meta.id)).unwrap();
    let locks = home.path().join("session_locks.json");
    std::fs::write(
        &locks,
        serde_json::json!({"version":1,"locked_sessions":[
            {"session_id":meta.short_id(),"locked_at":"2024-01-01"}
        ]})
        .to_string(),
    )
    .unwrap();
    assert!(store.delete_session(&meta.id).is_err());
    std::fs::write(&locks, "damaged").unwrap();
    assert!(
        store.delete_session(&meta.id).is_err(),
        "protection fails closed"
    );
    assert_eq!(
        std::fs::read(store.history_path(&meta.id)).unwrap(),
        history
    );
    store.delete_session_with_force(&meta.id, true).unwrap();
    assert!(!store.exists(&meta.id));
    let trash = std::fs::read_dir(store.base_dir().join(".trash"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        std::fs::read(trash.join(&meta.id).join("history.jsonl")).unwrap(),
        history
    );
    assert!(store.list_sessions().unwrap().is_empty());
}

#[test]
fn ux_contract_rejects_paths_ambiguous_ids_and_bad_fork_points() {
    let (_home, store, mut first) = fixture();
    first.id = "aaaaaaaa-0000".into();
    let mut second = first.clone();
    second.id = "aaaaaaaa-1111".into();
    store
        .create_session(&first, &[StoredMessage::user("one")])
        .unwrap();
    store.create_session(&second, &[]).unwrap();
    assert!(store.resolve_id("aaaaaaaa").is_err());
    for id in ["", "..", "../escape", "/tmp/escape", "x/y"] {
        assert!(store.load_meta(id).is_err());
        assert!(store.load_messages(id).is_err());
        assert!(store.delete_session_with_force(id, true).is_err());
    }
    let new_meta = SessionMeta::new("cortex", "test");
    assert!(
        store
            .fork_session(&first.id, &new_meta, Some("missing"))
            .is_err()
    );
    assert!(!store.exists(&new_meta.id));
}

#[test]
#[cfg(unix)]
fn ux_contract_rejects_session_and_history_symlinks() {
    let (home, store, meta) = fixture();
    store.ensure_base_dir().unwrap();
    let outside = home.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, store.session_dir(&meta.id)).unwrap();
    assert!(store.save_meta(&meta).is_err());
    assert!(
        store
            .append_message(&meta.id, &StoredMessage::user("no"))
            .is_err()
    );
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
    std::fs::remove_file(store.session_dir(&meta.id)).unwrap();
    store.create_session(&meta, &[]).unwrap();
    std::fs::remove_file(store.history_path(&meta.id)).unwrap();
    let sentinel = outside.join("work");
    std::fs::write(&sentinel, "user work").unwrap();
    std::os::unix::fs::symlink(&sentinel, store.history_path(&meta.id)).unwrap();
    assert!(
        store
            .append_message(&meta.id, &StoredMessage::user("no"))
            .is_err()
    );
    assert!(store.load_messages(&meta.id).is_err());
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "user work");
}

#[test]
#[cfg(unix)]
fn ux_contract_dangling_history_symlink_cannot_create_external_file() {
    let (home, store, meta) = fixture();
    store.create_session(&meta, &[]).unwrap();
    std::fs::remove_file(store.history_path(&meta.id)).unwrap();
    let outside = home.path().join("must-not-create");
    std::os::unix::fs::symlink(&outside, store.history_path(&meta.id)).unwrap();
    assert!(
        store
            .append_message(&meta.id, &StoredMessage::user("no"))
            .is_err()
    );
    assert!(!outside.exists());
}
