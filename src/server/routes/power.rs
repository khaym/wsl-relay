use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::AppState;
use crate::power::DEFAULT_TTL_SECS;

pub fn power_routes() -> Router<AppState> {
    Router::new().route(
        "/api/v1/power/inhibit",
        post(post_inhibit_handler)
            .get(get_inhibit_handler)
            .delete(delete_inhibit_handler),
    )
}

#[derive(Debug, Deserialize)]
struct InhibitRequest {
    #[serde(default = "default_ttl_seconds")]
    ttl_seconds: u64,
}

fn default_ttl_seconds() -> u64 {
    DEFAULT_TTL_SECS
}

fn check_enabled(state: &AppState) -> Result<(), (StatusCode, Json<Value>)> {
    if !state.config.is_operation_enabled("power") {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "power operation is disabled"})),
        ));
    }
    Ok(())
}

async fn post_inhibit_handler(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    check_enabled(&state)?;

    // The body is optional: an empty body means "all defaults".
    let body: &[u8] = if body.is_empty() { b"{}" } else { &body };
    let req: InhibitRequest = serde_json::from_slice(body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("invalid request body: {e}")})),
        )
    })?;

    if req.ttl_seconds == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "ttl_seconds must be at least 1"})),
        ));
    }

    let effective_ttl = state
        .power
        .acquire_or_renew(Duration::from_secs(req.ttl_seconds))
        .map_err(|e| {
            tracing::error!("Power inhibit failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "power inhibit failed"})),
            )
        })?;

    Ok(Json(json!({
        "ok": true,
        "ttl_seconds": effective_ttl.as_secs()
    })))
}

async fn get_inhibit_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    check_enabled(&state)?;

    let remaining = state.power.remaining();
    Ok(Json(json!({
        "active": remaining.is_some(),
        "remaining_seconds": remaining.map(|d| d.as_secs())
    })))
}

async fn delete_inhibit_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    check_enabled(&state)?;

    state.power.release();

    Ok(Json(json!({"ok": true})))
}
