pub mod routes;

use std::sync::Arc;

use axum::Router;

use crate::config::AppConfig;
use crate::notify::NotificationBackend;

#[derive(Clone)]
pub struct AppState {
    pub notifier: Arc<dyn NotificationBackend>,
    pub config: Arc<AppConfig>,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health_routes())
        .merge(routes::notify_routes())
        .with_state(state)
}
