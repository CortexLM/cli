//! Health check and metrics endpoints.

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};

use crate::state::AppState;

use super::types::HealthResponse;

/// Health check endpoint.
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HealthResponse>, StatusCode> {
    if !state.config.health_enabled {
        return Err(StatusCode::NOT_FOUND);
    }
    // This measures local readiness, not the availability of the coding API.
    let ready = state.config.validate().is_ok();
    if !ready {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok(Json(HealthResponse {
        status: "ready".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.uptime().as_secs(),
    }))
}

/// Get metrics.
pub async fn get_metrics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::state::MetricsSnapshot>, StatusCode> {
    if !state.config.metrics_enabled {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(state.get_metrics().await))
}
