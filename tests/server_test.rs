use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use wsl_relay::autostart::StubAutostart;
use wsl_relay::clipboard::{ClipboardBackend, StubClipboard};
use wsl_relay::config::AppConfig;
use wsl_relay::notify::{NotificationBackend, NotifyRequest, StubNotifier};
use wsl_relay::server::{AppState, build_router};

fn test_state() -> AppState {
    AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(StubAutostart),
        config: Arc::new(AppConfig::default()),
    }
}

struct FailingNotifier;

impl NotificationBackend for FailingNotifier {
    fn notify(&self, _req: &NotifyRequest) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("notification service unavailable"))
    }
}

#[tokio::test]
async fn health_returns_200_with_status_ok() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn notify_returns_200_with_valid_request() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/notify")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"title":"Test","body":"Hello","icon":"success"}"#,
                ))
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
async fn notify_returns_422_with_invalid_icon() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/notify")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"title":"Test","body":"Hello","icon":"unknown"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn notify_returns_403_when_disabled() {
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
                .method("POST")
                .uri("/api/v1/notify")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Test","body":"Hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn notify_returns_400_with_empty_body() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/notify")
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn notify_returns_400_with_malformed_json() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/notify")
                .header("content-type", "application/json")
                .body(Body::from("{invalid json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn notify_returns_422_with_missing_required_fields() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/notify")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn notify_returns_415_without_content_type() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/notify")
                .body(Body::from(r#"{"title":"Test","body":"Hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn notify_returns_500_when_notifier_fails() {
    let state = AppState {
        notifier: Arc::new(FailingNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(StubAutostart),
        config: Arc::new(AppConfig::default()),
    };
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/notify")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Test","body":"Hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("notification delivery failed")
    );
}

// --- Clipboard image tests ---

struct FailingClipboard;

impl ClipboardBackend for FailingClipboard {
    fn read_image(&self) -> anyhow::Result<Vec<u8>> {
        Err(anyhow::anyhow!("clipboard unavailable"))
    }
}

#[tokio::test]
async fn clipboard_image_returns_200_with_png_content_type() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/clipboard/image")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "image/png");

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let png_signature = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    assert_eq!(&body[..8], &png_signature);
}

#[tokio::test]
async fn clipboard_image_returns_403_when_disabled() {
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
                .uri("/api/v1/clipboard/image")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn clipboard_image_returns_500_when_backend_fails() {
    let state = AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(FailingClipboard),
        autostart: Arc::new(StubAutostart),
        config: Arc::new(AppConfig::default()),
    };
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/clipboard/image")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("clipboard read failed")
    );
}
