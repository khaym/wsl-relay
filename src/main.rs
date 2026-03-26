#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::info;

use wsl_relay::config::AppConfig;
use wsl_relay::server::{AppState, build_router};
use wsl_relay::tray::TrayBackend;

fn load_config() -> AppConfig {
    // Priority: WSL_RELAY_CONFIG env > %APPDATA%\wsl-relay\config.toml > defaults
    let config_path = std::env::var("WSL_RELAY_CONFIG")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(AppConfig::default_config_path);

    let config = match config_path {
        Some(p) => match std::fs::read_to_string(&p) {
            Ok(content) => AppConfig::from_toml_str(&content).unwrap_or_else(|e| {
                tracing::error!(
                    "Failed to parse config {}: {}, using defaults",
                    p.display(),
                    e
                );
                AppConfig::default()
            }),
            Err(_) => {
                // File not found is expected (user creates only when needed)
                AppConfig::default()
            }
        },
        None => AppConfig::default(),
    };

    // WSL_RELAY_PORT env overrides port from any source
    config.apply_port_env_override()
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

fn create_clipboard() -> Arc<dyn wsl_relay::clipboard::ClipboardBackend> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(wsl_relay::clipboard::WindowsClipboard)
    }
    #[cfg(not(target_os = "windows"))]
    {
        tracing::warn!("Running on non-Windows platform, clipboard will return stub data");
        Arc::new(wsl_relay::clipboard::StubClipboard)
    }
}

fn create_autostart() -> Arc<dyn wsl_relay::autostart::AutostartBackend> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(wsl_relay::autostart::WindowsAutostart)
    }
    #[cfg(not(target_os = "windows"))]
    {
        tracing::warn!("Running on non-Windows platform, autostart will be a no-op");
        Arc::new(wsl_relay::autostart::StubAutostart)
    }
}

fn create_tray() -> Box<dyn TrayBackend> {
    #[cfg(target_os = "windows")]
    {
        Box::new(wsl_relay::tray::WindowsTray)
    }
    #[cfg(not(target_os = "windows"))]
    {
        tracing::warn!("Running on non-Windows platform, system tray will be a no-op");
        Box::new(wsl_relay::tray::StubTray)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Register AUMID on Windows so notifications use our app identity
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = wsl_relay::notify::register_aumid() {
            tracing::error!("Failed to register AUMID: {}", e);
        }
    }

    let config = load_config();
    let addr = format!("127.0.0.1:{}", config.port);

    let state = AppState {
        notifier: create_notifier(),
        clipboard: create_clipboard(),
        autostart: create_autostart(),
        config: Arc::new(config),
    };

    let app = build_router(state);
    let listener = TcpListener::bind(&addr).await?;

    // Spawn the tray icon on a dedicated OS thread
    let (quit_tx, quit_rx) = std::sync::mpsc::sync_channel::<()>(1);
    let tray = create_tray();
    let tray_handle = std::thread::spawn(move || {
        if let Err(e) = tray.run(quit_tx) {
            tracing::error!("Tray error: {}", e);
        }
    });

    info!("wsl-relay listening on {}", addr);

    // Run server until quit signal (from tray) or Ctrl+C
    let quit_future = async {
        tokio::task::spawn_blocking(move || {
            quit_rx.recv().ok();
        })
        .await
        .ok();
    };

    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(e) = result {
                tracing::error!("Server error: {}", e);
            }
        }
        _ = quit_future => {
            info!("Quit signal received from tray, shutting down");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl+C received, shutting down");
        }
    }

    // Clean up: unpark the stub tray thread (no-op on Windows where the thread already exited)
    tray_handle.thread().unpark();
    let _ = tray_handle.join();

    Ok(())
}
