//! `/handoff` confirm: stay on this CLI, or follow live Cortex Cloud agents.

use cortex_engine::client::{AUTH_REQUIRED, CodeAgentClient, CodeSession};

use super::core::EventLoop;
use crate::interactive::builders::JobRow;

impl EventLoop {
    /// Stay closes the picker. Cloud lists (or creates) Code sessions — never
    /// the empty local `/jobs` picker.
    pub(super) async fn handle_handoff_confirm_choice(&mut self, item_id: &str) -> bool {
        if item_id != "cloud" {
            return false;
        }
        self.add_system_message("This CLI session stays here (Chat · Code · Bot).");
        match self.load_cloud_job_rows().await {
            Ok(rows) => {
                let interactive = crate::interactive::builders::build_jobs_picker(&rows);
                self.app_state.enter_interactive_mode(interactive);
                true
            }
            Err(message) => {
                self.app_state.exit_interactive_mode();
                self.add_system_message(&message);
                false
            }
        }
    }

    async fn load_cloud_job_rows(&self) -> Result<Vec<JobRow>, String> {
        let client = self.code_agent_client()?;
        let sessions = list_or_create_cloud_sessions(&client, self.handoff_session_title()).await?;
        if sessions.iter().any(|session| session.id.is_empty()) {
            return Err("The coding service is temporarily unavailable".into());
        }
        Ok(sessions.iter().map(JobRow::from_code_session).collect())
    }

    fn handoff_session_title(&self) -> Option<String> {
        self.cortex_session
            .as_ref()
            .map(crate::session::CortexSession::title)
            .filter(|title| !title.is_empty())
    }

    fn code_agent_client(&self) -> Result<CodeAgentClient, String> {
        let token = cloud_handoff_token().ok_or_else(|| AUTH_REQUIRED.to_string())?;
        Ok(CodeAgentClient::new(self.code_agent_api_url(), Some(token)))
    }

    fn code_agent_api_url(&self) -> Option<String> {
        if let Some(pm) = &self.provider_manager
            && let Ok(manager) = pm.try_read()
        {
            let url = manager.api_url();
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
        std::env::var("CORTEX_API_URL")
            .ok()
            .filter(|url| !url.is_empty())
    }
}

fn cloud_handoff_token() -> Option<String> {
    std::env::var("CORTEX_AUTH_TOKEN")
        .ok()
        .filter(|token| !token.is_empty())
        .or_else(|| {
            std::env::var("CORTEX_API_KEY")
                .ok()
                .filter(|token| !token.is_empty())
        })
        .or_else(cortex_login::get_auth_token)
}

async fn list_or_create_cloud_sessions(
    client: &CodeAgentClient,
    title: Option<String>,
) -> Result<Vec<CodeSession>, String> {
    let sessions = client
        .list_sessions()
        .await
        .map_err(|err| err.user_friendly_message())?;
    if !sessions.is_empty() {
        return Ok(sessions);
    }
    let created = client
        .create_session(title.as_deref())
        .await
        .map_err(|err| err.user_friendly_message())?;
    Ok(vec![created])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::session::{CortexSession, SessionStorage};
    use serial_test::serial;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn isolate_auth_env() {
        unsafe {
            std::env::remove_var("CORTEX_AUTH_TOKEN");
            std::env::remove_var("CORTEX_API_KEY");
        }
    }

    fn fixture() -> (tempfile::TempDir, EventLoop) {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStorage::with_dir(temp.path().join("sessions"));
        let session = CortexSession::with_storage("cortex", "test", store).unwrap();
        let event_loop = EventLoop::new(AppState::new()).with_cortex_session(session);
        (temp, event_loop)
    }

    #[tokio::test]
    #[serial]
    async fn cloud_without_auth_does_not_open_local_jobs() {
        isolate_auth_env();
        unsafe {
            std::env::set_var("CORTEX_API_URL", "http://127.0.0.1:9");
        }
        let (_temp, mut runner) = fixture();
        let keep = runner.handle_handoff_confirm_choice("cloud").await;
        unsafe {
            std::env::remove_var("CORTEX_API_URL");
        }
        assert!(!keep);
        assert!(runner.app_state.get_interactive_state().is_none());
        let last = runner
            .app_state
            .messages
            .last()
            .expect("auth copy")
            .content
            .clone();
        assert!(
            last.contains("Not signed in") || last.contains("temporarily unavailable"),
            "{last}"
        );
        assert!(!last.to_ascii_lowercase().contains("reqwest"), "{last}");
    }

    #[tokio::test]
    #[serial]
    async fn stay_does_not_open_jobs() {
        let (_temp, mut runner) = fixture();
        assert!(!runner.handle_handoff_confirm_choice("stay").await);
        assert!(runner.app_state.get_interactive_state().is_none());
    }

    #[tokio::test]
    #[serial]
    async fn cloud_lists_live_code_sessions() {
        isolate_auth_env();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/code/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{
                    "id": "sess-live-1",
                    "runtime": "cloud",
                    "state": "running",
                    "title": "lock boards"
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/code/sessions"))
            .respond_with(ResponseTemplate::new(599))
            .expect(0)
            .mount(&server)
            .await;

        unsafe {
            std::env::set_var("CORTEX_API_KEY", "handoff-fixture-token");
            std::env::set_var("CORTEX_API_URL", server.uri());
        }
        let (_temp, mut runner) = fixture();
        let keep = runner.handle_handoff_confirm_choice("cloud").await;
        unsafe {
            std::env::remove_var("CORTEX_API_KEY");
            std::env::remove_var("CORTEX_API_URL");
        }
        assert!(keep);
        let jobs = runner
            .app_state
            .get_interactive_state()
            .expect("cloud rows open the jobs picker");
        assert_eq!(jobs.items[0].id, "sess-live-1");
        assert!(!jobs.items[0].disabled);
        assert!(!jobs.title.contains("none running"), "{}", jobs.title);
        assert_eq!(runner.app_state.active_subagents.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn cloud_creates_a_session_when_the_list_is_empty() {
        isolate_auth_env();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/code/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": []
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/code/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "sess-created-9",
                "runtime": "cloud",
                "title": "test"
            })))
            .expect(1)
            .mount(&server)
            .await;

        unsafe {
            std::env::set_var("CORTEX_API_KEY", "handoff-fixture-token");
            std::env::set_var("CORTEX_API_URL", server.uri());
        }
        let (_temp, mut runner) = fixture();
        let keep = runner.handle_handoff_confirm_choice("cloud").await;
        unsafe {
            std::env::remove_var("CORTEX_API_KEY");
            std::env::remove_var("CORTEX_API_URL");
        }
        assert!(keep);
        let jobs = runner
            .app_state
            .get_interactive_state()
            .expect("created session is listed");
        assert_eq!(jobs.items[0].id, "sess-created-9");
    }

    #[tokio::test]
    #[serial]
    async fn cloud_api_outage_is_product_copy_and_not_local_jobs() {
        isolate_auth_env();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/code/sessions"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;

        unsafe {
            std::env::set_var("CORTEX_API_KEY", "handoff-fixture-token");
            std::env::set_var("CORTEX_API_URL", server.uri());
        }
        let (_temp, mut runner) = fixture();
        let keep = runner.handle_handoff_confirm_choice("cloud").await;
        unsafe {
            std::env::remove_var("CORTEX_API_KEY");
            std::env::remove_var("CORTEX_API_URL");
        }
        assert!(!keep);
        assert!(runner.app_state.get_interactive_state().is_none());
        let last = runner
            .app_state
            .messages
            .last()
            .expect("outage copy")
            .content
            .clone();
        assert!(
            last.contains("The coding service is temporarily unavailable"),
            "{last}"
        );
    }
}
