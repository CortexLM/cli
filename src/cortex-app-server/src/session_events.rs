//! Single-reader engine event fan-out. Sequence IDs are live-only, not replay cursors.
use super::SessionError;
use crate::storage::{SessionStorage, StoredMessage};
use cortex_protocol::{Event, EventMsg};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{Mutex, broadcast};

#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub sequence: u64,
    pub event: Event,
}
#[derive(Default)]
struct Control {
    turn: Option<String>,
    failed: bool,
    approvals: HashSet<String>,
    sequence: u64,
    closed: bool,
}
pub(super) struct EventHub {
    session_id: String,
    sender: broadcast::Sender<SessionEvent>,
    control: Mutex<Control>,
}
impl EventHub {
    pub fn new(session_id: String) -> Self {
        // ponytail: retain only a 256-event live window; add durable cursors before advertising replay.
        let (sender, _) = broadcast::channel(256);
        Self {
            session_id,
            sender,
            control: Mutex::new(Control::default()),
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.sender.subscribe()
    }
    pub async fn status(&self) -> &'static str {
        let state = self.control.lock().await;
        if state.closed {
            "closed"
        } else if state.turn.is_some() {
            "processing"
        } else {
            "ready"
        }
    }
    pub async fn start_turn(&self, turn: &str) -> Result<(), SessionError> {
        let mut state = self.control.lock().await;
        if state.closed || state.turn.is_some() {
            return Err(SessionError::InvalidState(
                "Session is closed or already has an active turn".into(),
            ));
        }
        state.turn = Some(turn.into());
        state.failed = false;
        Ok(())
    }
    pub async fn require_idle(&self) -> Result<(), SessionError> {
        let state = self.control.lock().await;
        if state.closed || state.turn.is_some() {
            return Err(SessionError::InvalidState("Session is not idle".into()));
        }
        Ok(())
    }
    pub async fn require_turn(&self) -> Result<(), SessionError> {
        if self.control.lock().await.turn.is_none() {
            return Err(SessionError::InvalidState("No active turn".into()));
        }
        Ok(())
    }
    pub async fn clear_turn(&self) {
        let mut state = self.control.lock().await;
        state.turn = None;
        state.approvals.clear();
    }
    pub async fn take_approval(&self, id: &str) -> Result<(), SessionError> {
        let mut state = self.control.lock().await;
        if state.turn.is_none() || !state.approvals.remove(id) {
            return Err(SessionError::InvalidState(
                "No pending approval with that call ID".into(),
            ));
        }
        Ok(())
    }
    pub async fn close(&self) {
        self.publish(Event {
            id: String::new(),
            msg: EventMsg::ShutdownComplete,
        })
        .await;
    }
    async fn publish(&self, event: Event) {
        let mut state = self.control.lock().await;
        if state.closed {
            return;
        }
        // The engine ends a failed turn with Error and emits nothing further for it,
        // so Error must release the turn or the session wedges in `processing`.
        let terminal = matches!(
            event.msg,
            EventMsg::TaskComplete(_)
                | EventMsg::TurnAborted(_)
                | EventMsg::ShutdownComplete
                | EventMsg::Error(_)
        );
        // Error followed by TaskComplete is not a successful second terminal outcome.
        let suppress = matches!(event.msg, EventMsg::TaskComplete(_))
            && (state.failed || state.turn.is_none());
        if matches!(event.msg, EventMsg::Error(_)) {
            state.failed = true;
        }
        if let EventMsg::ExecApprovalRequest(ref approval) = event.msg {
            if state.turn.is_some() {
                state.approvals.insert(approval.call_id.clone());
            }
        }
        if matches!(event.msg, EventMsg::ShutdownComplete) {
            state.closed = true;
        }
        if !suppress {
            state.sequence += 1;
            let _ = self.sender.send(SessionEvent {
                session_id: self.session_id.clone(),
                turn_id: state.turn.clone(),
                sequence: state.sequence,
                event,
            });
        }
        if terminal {
            state.turn = None;
            state.approvals.clear();
        }
    }
}
pub(super) fn spawn_forwarder(
    receiver: async_channel::Receiver<Event>,
    hub: Arc<EventHub>,
    storage: Arc<SessionStorage>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            let stored = match &event.msg {
                EventMsg::UserMessage(e) => Some(("user", &e.message)),
                EventMsg::AgentMessage(e) => Some(("assistant", &e.message)),
                _ => None,
            };
            if let Some((role, content)) = stored {
                let message = StoredMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    role: role.into(),
                    content: content.clone(),
                    timestamp: chrono::Utc::now().timestamp(),
                    tool_calls: vec![],
                };
                if storage.append_message(&hub.session_id, &message).is_err() {
                    tracing::warn!("Session history could not be persisted");
                }
            }
            hub.publish(event).await;
        }
        hub.close().await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(msg: EventMsg) -> Event {
        Event {
            id: "engine-id".into(),
            msg,
        }
    }
    #[tokio::test]
    async fn two_subscribers_get_identical_ordered_events_across_turns() {
        let hub = EventHub::new("session".into());
        let (mut a, mut b) = (hub.subscribe(), hub.subscribe());
        for turn in ["turn-a", "turn-b"] {
            hub.start_turn(turn).await.unwrap();
            assert!(hub.start_turn("concurrent").await.is_err());
            hub.publish(event(EventMsg::TaskStarted(
                cortex_protocol::TaskStartedEvent {
                    model_context_window: None,
                },
            )))
            .await;
            hub.publish(event(EventMsg::TaskComplete(
                cortex_protocol::TaskCompleteEvent {
                    last_agent_message: None,
                },
            )))
            .await;
            for _ in 0..2 {
                let (one, two) = (a.recv().await.unwrap(), b.recv().await.unwrap());
                assert_eq!(one.sequence, two.sequence);
                assert_eq!(one.turn_id.as_deref(), Some(turn));
                assert_eq!(one.session_id, "session");
            }
        }
        assert!(hub.require_turn().await.is_err());
        assert!(hub.take_approval("unrequested").await.is_err());
    }
    #[tokio::test]
    async fn error_is_not_followed_by_success_and_closed_is_final() {
        let hub = EventHub::new("session".into());
        let mut rx = hub.subscribe();
        hub.start_turn("turn").await.unwrap();
        hub.publish(event(EventMsg::Error(cortex_protocol::ErrorEvent {
            message: "fixture failure".into(),
            cortex_error_info: None,
        })))
        .await;
        assert!(matches!(
            rx.recv().await.unwrap().event.msg,
            EventMsg::Error(_)
        ));
        hub.publish(event(EventMsg::TaskComplete(
            cortex_protocol::TaskCompleteEvent {
                last_agent_message: None,
            },
        )))
        .await;
        assert!(rx.try_recv().is_err());
        hub.close().await;
        assert!(matches!(
            rx.recv().await.unwrap().event.msg,
            EventMsg::ShutdownComplete
        ));
        assert!(hub.start_turn("new").await.is_err());
    }
}
