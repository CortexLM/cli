//! Persistent session records are owned resources, not a separate authorization bypass.
use crate::{
    auth::{AuthResult, session_principal},
    error::{AppError, AppResult},
    state::AppState,
    storage::{StoredMessage, StoredSession},
};
use axum::{
    Extension, Json,
    extract::{Path, State},
};
use std::sync::Arc;

pub(crate) fn load_owned(
    state: &AppState,
    id: &str,
    owner: Option<&str>,
) -> AppResult<StoredSession> {
    uuid::Uuid::parse_str(id).map_err(|_| AppError::NotFound("Session not found".into()))?;
    let session = state
        .cli_sessions
        .storage()
        .load_session(id)
        .map_err(|_| AppError::NotFound("Session not found".into()))?;
    if session.user_id.as_deref() != owner {
        return Err(AppError::NotFound("Session not found".into()));
    }
    Ok(session)
}
pub async fn list_stored_sessions(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
) -> AppResult<Json<Vec<StoredSession>>> {
    let owner = session_principal(auth.as_ref().map(|a| &a.0), state.config.auth.enabled)?;
    let sessions = state
        .cli_sessions
        .storage()
        .list_sessions()
        .map_err(|_| AppError::Internal("Session storage unavailable".into()))?;
    Ok(Json(
        sessions
            .into_iter()
            .filter(|s| s.user_id == owner)
            .collect(),
    ))
}
pub async fn get_stored_session(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
) -> AppResult<Json<StoredSession>> {
    let owner = session_principal(auth.as_ref().map(|a| &a.0), state.config.auth.enabled)?;
    Ok(Json(load_owned(&state, &id, owner.as_deref())?))
}
pub async fn delete_stored_session(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let owner = session_principal(auth.as_ref().map(|a| &a.0), state.config.auth.enabled)?;
    load_owned(&state, &id, owner.as_deref())?;
    if state.cli_sessions.get_session(&id).await.is_some() {
        return Err(AppError::Conflict(
            "Close the live session before deleting its history".into(),
        ));
    }
    state
        .cli_sessions
        .storage()
        .delete_session(&id)
        .map_err(|_| AppError::Internal("Session storage unavailable".into()))?;
    Ok(Json(serde_json::json!({"deleted":true})))
}
pub async fn get_session_history(
    State(state): State<Arc<AppState>>,
    auth: Option<Extension<AuthResult>>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<StoredMessage>>> {
    let owner = session_principal(auth.as_ref().map(|a| &a.0), state.config.auth.enabled)?;
    load_owned(&state, &id, owner.as_deref())?;
    Ok(Json(
        state
            .cli_sessions
            .storage()
            .read_history(&id)
            .map_err(|_| AppError::Internal("Session storage unavailable".into()))?,
    ))
}
