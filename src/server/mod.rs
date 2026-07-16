pub mod routes;

use std::sync::Arc;

use axum::Router;

use crate::autostart::AutostartBackend;
use crate::clipboard::ClipboardBackend;
use crate::config::AppConfig;
use crate::notify::NotificationBackend;
use crate::power::PowerInhibitManager;

#[derive(Clone)]
pub struct AppState {
    pub notifier: Arc<dyn NotificationBackend>,
    pub clipboard: Arc<dyn ClipboardBackend>,
    pub autostart: Arc<dyn AutostartBackend>,
    // Stateful orchestrator (holds the live inhibit slot), unlike the
    // stateless per-call backends above.
    pub power: Arc<PowerInhibitManager>,
    pub config: Arc<AppConfig>,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health_routes())
        .merge(routes::notify_routes())
        .merge(routes::clipboard_routes())
        .merge(routes::autostart_routes())
        .merge(routes::power_routes())
        .with_state(state)
}
