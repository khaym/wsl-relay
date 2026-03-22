use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::info;

use wsl_relay::config::AppConfig;
use wsl_relay::server::{build_router, AppState};

fn load_config() -> AppConfig {
    let path = std::env::var("WSL_RELAY_CONFIG").ok();
    match path {
        Some(p) => match std::fs::read_to_string(&p) {
            Ok(content) => AppConfig::from_toml_str(&content).unwrap_or_else(|e| {
                tracing::error!("Failed to parse config {}: {}, using defaults", p, e);
                AppConfig::default()
            }),
            Err(e) => {
                tracing::error!("Failed to read config {}: {}, using defaults", p, e);
                AppConfig::default()
            }
        },
        None => AppConfig::default(),
    }
}

fn create_notifier() -> Arc<dyn wsl_relay::notify::NotificationBackend> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(wsl_relay::notify::WindowsNotifier)
    }
    #[cfg(not(target_os = "windows"))]
    {
        tracing::warn!("Running on non-Windows platform, notifications will be no-ops");
        Arc::new(wsl_relay::notify::StubNotifier)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = load_config();
    let addr = format!("127.0.0.1:{}", config.port);

    let state = AppState {
        notifier: create_notifier(),
        config: Arc::new(config),
    };

    let app = build_router(state);
    let listener = TcpListener::bind(&addr).await?;

    info!("wsl-relay listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
