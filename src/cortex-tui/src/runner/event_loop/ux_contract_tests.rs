use super::EventLoop;
use crate::actions::{ActionContext, KeyAction};
use crate::app::AppState;
use crate::commands::{CommandResult, ModalType};
use crate::modal::ModalAction;
use crate::session::{CortexSession, SessionStorage, StoredMessage};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

fn fixture() -> (tempfile::TempDir, EventLoop) {
    let temp = tempfile::tempdir().unwrap();
    let store = SessionStorage::with_dir(temp.path().join("sessions"));
    let session = CortexSession::with_storage("cortex", "test", store).unwrap();
    let event_loop = EventLoop::new(AppState::new()).with_cortex_session(session);
    (temp, event_loop)
}

fn render(event_loop: &mut EventLoop, width: u16, height: u16) -> String {
    event_loop.app_state.terminal_size = (width, height);
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    crate::views::minimal_session::MinimalSessionView::new(&event_loop.app_state)
        .render(area, &mut buffer);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_views(event_loop: &mut EventLoop, expected: &str) {
    for (width, height) in [(40, 12), (120, 40)] {
        let text = render(event_loop, width, height);
        assert!(
            text.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains(expected),
            "{width}x{height}: expected {expected:?}\n{text}"
        );
    }
}

#[tokio::test]
async fn ux_contract_new_fork_resume_and_palette_share_persisted_identity() {
    let (_temp, mut runner) = fixture();
    let session = runner.cortex_session.as_mut().unwrap();
    session
        .try_add_message_raw(StoredMessage::system("rules"))
        .unwrap();
    session
        .try_add_message_raw(StoredMessage::user("source prompt"))
        .unwrap();
    session
        .try_add_message_raw(StoredMessage::assistant("source answer"))
        .unwrap();
    let id = session.id().to_string();
    let storage = session.storage().clone();
    let original = std::fs::read(storage.history_path(&id)).unwrap();
    runner
        .process_modal_action(ModalAction::ExecuteCommand("fork".into()))
        .await;
    let fork_id = runner.cortex_session.as_ref().unwrap().id().to_string();
    assert_ne!(fork_id, id);
    assert_eq!(runner.app_state.session_id.unwrap().to_string(), fork_id);
    assert_eq!(runner.cortex_session.as_ref().unwrap().message_count(), 3);
    assert_eq!(std::fs::read(storage.history_path(&id)).unwrap(), original);
    assert_views(&mut runner, "Fork created");

    runner
        .app_state
        .queue_message("must not cross sessions".into());
    runner.handle_action(KeyAction::NewSession).await.unwrap();
    let new_id = runner.cortex_session.as_ref().unwrap().id().to_string();
    assert_ne!(new_id, fork_id);
    assert_eq!(
        runner
            .cortex_session
            .as_ref()
            .unwrap()
            .messages_for_api()
            .len(),
        0
    );
    assert!(!runner.app_state.has_queued_messages());
    assert_views(&mut runner, "New session");

    runner
        .process_modal_action(ModalAction::SelectSession(id.clone().into()))
        .await;
    assert_eq!(runner.app_state.session_id.unwrap().to_string(), id);
    assert_eq!(
        runner.app_state.messages[0].role,
        cortex_core::widgets::MessageRole::System
    );
    assert_eq!(runner.app_state.messages[1].content, "source prompt");
    assert_views(&mut runner, "Session resumed");
}

#[tokio::test]
async fn ux_contract_composer_keys_and_unknown_slash_never_reach_model() {
    let (_temp, mut runner) = fixture();
    runner.app_state.input.set_text("hello");
    for (key, expected) in [
        (
            KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
            "hello\n",
        ),
        (KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL), ""),
    ] {
        let action = runner.action_mapper.get_action(key, ActionContext::Input);
        runner.handle_action(action).await.unwrap();
        assert_eq!(runner.app_state.input.text(), expected);
    }
    runner.app_state.input.set_text("composer marker");
    assert_views(&mut runner, "composer marker");
    runner.app_state.input.set_text("/no-such-command");
    runner.handle_action(KeyAction::Submit).await.unwrap();
    assert_eq!(runner.cortex_session.as_ref().unwrap().message_count(), 0);
    assert_views(&mut runner, "Unknown command");
}

#[tokio::test]
async fn ux_contract_initial_prompt_dispatch_preserves_draft_on_unavailable_service() {
    let (_temp, mut runner) = fixture();
    let error = runner
        .submit_initial_prompt("startup prompt".into())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "The coding service is temporarily unavailable"
    );
    assert_eq!(runner.app_state.input.text(), "startup prompt");
    assert_eq!(runner.cortex_session.as_ref().unwrap().message_count(), 0);
    // The narrow transcript wraps, so verify the product message by its stable line.
    assert_views(&mut runner, "coding service");
}

#[tokio::test]
async fn ux_contract_metadata_and_export_survive_restart_without_overwrite() {
    let (_temp, mut runner) = fixture();
    runner.handle_set_value("session_name", "界🙂".repeat(30).as_str());
    runner.handle_set_value("favorite", "true");
    let session = runner.cortex_session.as_ref().unwrap();
    let store = session.storage().clone();
    let id = session.id().to_string();
    assert!(store.load_meta(&id).unwrap().favorite);
    assert_eq!(store.load_meta(&id).unwrap().title, session.meta.title);
    runner
        .process_modal_action(ModalAction::ExecuteCommand("export json".into()))
        .await;
    let export = std::fs::read_dir(store.base_dir().join(".exports"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let bytes = std::fs::read(&export).unwrap();
    let document: cortex_engine::rollout::local::SessionDocument =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document.meta.id, id);
    assert_views(&mut runner, "Session exported");
    assert!(super::local_workflows::write_export(&export, "overwrite").is_err());
    assert_eq!(std::fs::read(export).unwrap(), bytes);
}

#[tokio::test]
async fn ux_contract_rewind_and_redo_leave_workspace_and_source_unchanged() {
    let (temp, mut runner) = fixture();
    let work = temp.path().join("user-work.txt");
    std::fs::write(&work, "uncommitted user edits").unwrap();
    let session = runner.cortex_session.as_mut().unwrap();
    for text in ["first", "second"] {
        session
            .try_add_message_raw(StoredMessage::user(text))
            .unwrap();
        session
            .try_add_message_raw(StoredMessage::assistant("reply"))
            .unwrap();
    }
    let id = session.id().to_string();
    runner.handle_async_command("undo").await.unwrap();
    assert_ne!(runner.cortex_session.as_ref().unwrap().id(), id);
    assert_eq!(runner.cortex_session.as_ref().unwrap().message_count(), 2);
    assert_eq!(
        std::fs::read_to_string(&work).unwrap(),
        "uncommitted user edits"
    );
    assert_views(&mut runner, "Conversation");
    runner.handle_async_command("redo").await.unwrap();
    assert_eq!(runner.cortex_session.as_ref().unwrap().id(), id);
    assert_eq!(runner.cortex_session.as_ref().unwrap().message_count(), 4);
    assert_views(&mut runner, "Original conversation");
}

#[tokio::test]
async fn ux_contract_failed_resume_and_unsaved_session_keep_current_context() {
    let (_temp, mut runner) = fixture();
    let id = runner.cortex_session.as_ref().unwrap().id().to_string();
    runner
        .handle_command_result(CommandResult::ResumeSession("../escape".into()))
        .await
        .unwrap();
    assert_eq!(runner.cortex_session.as_ref().unwrap().id(), id);
    assert_views(&mut runner, "Invalid session ID");
    let session = runner.cortex_session.as_mut().unwrap();
    let path = session.storage().history_path(&id);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    session.add_user_message("unsaved draft");
    assert!(session.is_modified());
    runner.handle_open_modal(ModalType::Fork).await;
    assert_eq!(runner.cortex_session.as_ref().unwrap().id(), id);
    assert_views(&mut runner, "unsaved changes");
}

#[tokio::test]
async fn ux_contract_diff_runs_real_git_without_mutating_user_work() {
    let temp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(temp.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init", "--quiet"]);
    // No commit/identity changes: review against the empty tree object.
    run(&["hash-object", "-t", "tree", "--stdin", "-w"]);
    std::fs::write(temp.path().join("file.txt"), "new content\n").unwrap();
    run(&["add", "file.txt"]);
    let diff = super::local_workflows::read_local_diff(
        temp.path(),
        "review:uncommitted:base=4b825dc642cb6eb9a060e54bf8d69288fbee4904",
    )
    .await
    .unwrap();
    assert!(diff.contains("+new content"));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("file.txt")).unwrap(),
        "new content\n"
    );
    assert!(
        super::local_workflows::read_local_diff(temp.path(), "diff:../outside")
            .await
            .is_err()
    );
    let (_home, mut runner) = fixture();
    runner.add_system_message(&diff);
    assert_views(&mut runner, "new content");
}

#[tokio::test]
async fn ux_contract_picker_export_selector_and_limitations_render() {
    let (_temp, mut runner) = fixture();
    runner
        .change_session_metadata("session_name", "picker-session")
        .unwrap();
    runner
        .process_modal_action(ModalAction::ExecuteCommand("sessions".into()))
        .await;
    assert_eq!(runner.modal_stack.len(), 1);
    for (width, height) in [(40, 12), (120, 40)] {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        runner
            .modal_stack
            .current()
            .unwrap()
            .render(area, &mut buffer);
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("picker-session"), "{text}");
    }
    runner.modal_stack.clear();
    runner.handle_open_modal(ModalType::Export(None)).await;
    assert!(runner.app_state.get_interactive_state().is_some());
    runner
        .handle_interactive_selection(
            crate::interactive::InteractiveAction::Custom("export".into()),
            "json".into(),
            vec![],
        )
        .await;
    runner.app_state.exit_interactive_mode();
    assert_views(&mut runner, "Session exported");
    runner.handle_set_value("unimplemented_setting", "true");
    assert!(
        !runner
            .app_state
            .settings
            .contains_key("unimplemented_setting")
    );
    assert_views(&mut runner, "unsupported");
    runner
        .handle_action(KeyAction::OpenExternalEditor)
        .await
        .unwrap();
    assert_views(&mut runner, "External editor");
}

#[test]
fn ux_contract_unicode_auto_title_and_concurrent_append_are_lossless() {
    let (_temp, mut runner) = fixture();
    let session = runner.cortex_session.as_mut().unwrap();
    let prompt = "界🙂e\u{301}".repeat(30);
    session.add_user_message(&prompt);
    session.auto_title();
    assert!(!session.is_modified());
    assert!(session.meta.title.as_ref().unwrap().ends_with("..."));
    let stored = session.storage().load_meta(session.id()).unwrap();
    assert_eq!(stored.title, session.meta.title);
    session
        .storage()
        .append_message(session.id(), &StoredMessage::assistant("other writer"))
        .unwrap();
    session.set_title("kept title");
    session.save().unwrap();
    assert_eq!(session.messages().last().unwrap().content, "other writer");
    assert_eq!(
        session.storage().load_messages(session.id()).unwrap().len(),
        2
    );
    runner.app_state.input.set_text("界🙂 Unicode input");
    assert_views(&mut runner, "Unicode input");
}

#[tokio::test]
async fn ux_contract_me_profile_applies_off_the_render_path() {
    let (_temp, mut runner) = fixture();
    let handle = tokio::spawn(async {
        Some(cortex_engine::client::MeProfile {
            name: Some("Ada Lovelace".into()),
            email: Some("ada@example.com".into()),
            org_name: Some("Analytical Engines".into()),
        })
    });
    runner.me_profile_task = Some(handle);
    let mut applied = false;
    for _ in 0..50 {
        if runner.apply_pending_me_profile().await {
            applied = true;
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        applied,
        "finished /v1/me task must apply without blocking startup"
    );
    assert_eq!(runner.app_state.user_name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(
        runner.app_state.org_name.as_deref(),
        Some("Analytical Engines")
    );
}
