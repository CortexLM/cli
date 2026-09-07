//! Compatibility completions are not a supported inference route.
use super::types::{ChatCompletionRequest, ChatCompletionResponse};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::{Json, extract::State};
use std::sync::Arc;

pub async fn chat_completions(
    State(_state): State<Arc<AppState>>,
    Json(_req): Json<ChatCompletionRequest>,
) -> AppResult<Json<ChatCompletionResponse>> {
    Err(AppError::NotImplemented(
        "Use the CLI session turn API".into(),
    ))
}
