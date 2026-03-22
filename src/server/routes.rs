use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use super::AppState;
use crate::notify::NotifyRequest;

pub fn health_routes() -> Router<AppState> {
    Router::new().route("/api/v1/health", get(health_handler))
}

pub fn notify_routes() -> Router<AppState> {
    Router::new().route("/api/v1/notify", post(notify_handler))
}

async fn health_handler() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn notify_handler(
    State(state): State<AppState>,
    Json(req): Json<NotifyRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state.config.is_operation_enabled("notify") {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "notify operation is disabled"})),
        ));
    }

    let notifier = state.notifier.clone();
    tokio::task::spawn_blocking(move || notifier.notify(&req))
        .await
        .map_err(|e| {
            tracing::error!("Notification task panicked: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
        })?
        .map_err(|e| {
            tracing::error!("Notification failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "notification delivery failed"})),
            )
        })?;

    Ok(Json(json!({"ok": true})))
}
