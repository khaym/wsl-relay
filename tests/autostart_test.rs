use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use wsl_relay::autostart::{AutostartBackend, StubAutostart};
use wsl_relay::clipboard::StubClipboard;
use wsl_relay::config::AppConfig;
use wsl_relay::notify::StubNotifier;
use wsl_relay::server::{AppState, build_router};

fn test_state() -> AppState {
    AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(StubAutostart),
        config: Arc::new(AppConfig::default()),
    }
}

struct FailingAutostart;

impl AutostartBackend for FailingAutostart {
    fn enable(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("registry write failed"))
    }

    fn disable(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("registry delete failed"))
    }

    fn is_enabled(&self) -> anyhow::Result<bool> {
        Err(anyhow::anyhow!("registry read failed"))
    }
}

// --- Unit tests ---

#[test]
fn stub_enable_returns_ok() {
    let stub = StubAutostart;
    assert!(stub.enable().is_ok());
}

#[test]
fn stub_disable_returns_ok() {
    let stub = StubAutostart;
    assert!(stub.disable().is_ok());
}

#[test]
fn stub_is_enabled_returns_false() {
    let stub = StubAutostart;
    assert!(!stub.is_enabled().unwrap());
}

// --- API tests ---

#[tokio::test]
async fn get_autostart_returns_200_with_enabled_false() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["enabled"], false);
}

#[tokio::test]
async fn put_autostart_returns_200() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn delete_autostart_returns_200() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn get_autostart_returns_403_when_disabled() {
    let state = AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(StubAutostart),
        config: Arc::new(AppConfig::from_toml_str(r#"enabled_operations = ["health"]"#).unwrap()),
    };
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn put_autostart_returns_403_when_disabled() {
    let state = AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(StubAutostart),
        config: Arc::new(AppConfig::from_toml_str(r#"enabled_operations = ["health"]"#).unwrap()),
    };
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_autostart_returns_403_when_disabled() {
    let state = AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(StubAutostart),
        config: Arc::new(AppConfig::from_toml_str(r#"enabled_operations = ["health"]"#).unwrap()),
    };
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn put_autostart_returns_500_when_backend_fails() {
    let state = AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(FailingAutostart),
        config: Arc::new(AppConfig::default()),
    };
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "autostart enable failed");
}

#[tokio::test]
async fn get_autostart_returns_500_when_backend_fails() {
    let state = AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(FailingAutostart),
        config: Arc::new(AppConfig::default()),
    };
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "autostart status check failed");
}

#[tokio::test]
async fn delete_autostart_returns_500_when_backend_fails() {
    let state = AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(FailingAutostart),
        config: Arc::new(AppConfig::default()),
    };
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/autostart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "autostart disable failed");
}
