use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use wsl_relay::autostart::StubAutostart;
use wsl_relay::clipboard::StubClipboard;
use wsl_relay::config::AppConfig;
use wsl_relay::notify::StubNotifier;
use wsl_relay::power::{
    DEFAULT_TTL_SECS, MAX_TTL_SECS, PowerBackend, PowerInhibitManager, PowerRequestHandle,
    StubPowerBackend,
};
use wsl_relay::server::{AppState, build_router};

/// Test double that counts backend calls so tests can assert how many
/// OS-level power requests were created and cleared.
struct CountingBackend {
    creates: Arc<AtomicUsize>,
    clears: Arc<AtomicUsize>,
}

struct CountingHandle {
    clears: Arc<AtomicUsize>,
}

impl PowerRequestHandle for CountingHandle {
    fn clear(&mut self) -> anyhow::Result<()> {
        self.clears.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl PowerBackend for CountingBackend {
    fn create_request(&self, _reason: &str) -> anyhow::Result<Box<dyn PowerRequestHandle>> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(CountingHandle {
            clears: self.clears.clone(),
        }))
    }
}

struct FailingPowerBackend;

impl PowerBackend for FailingPowerBackend {
    fn create_request(&self, _reason: &str) -> anyhow::Result<Box<dyn PowerRequestHandle>> {
        Err(anyhow::anyhow!("power request creation failed"))
    }
}

/// Creates requests fine, but clearing them fails.
struct FailingClearBackend;

struct FailingClearHandle;

impl PowerRequestHandle for FailingClearHandle {
    fn clear(&mut self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("power request clear failed"))
    }
}

impl PowerBackend for FailingClearBackend {
    fn create_request(&self, _reason: &str) -> anyhow::Result<Box<dyn PowerRequestHandle>> {
        Ok(Box::new(FailingClearHandle))
    }
}

fn counting_manager() -> (Arc<PowerInhibitManager>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let creates = Arc::new(AtomicUsize::new(0));
    let clears = Arc::new(AtomicUsize::new(0));
    let manager = PowerInhibitManager::new(Arc::new(CountingBackend {
        creates: creates.clone(),
        clears: clears.clone(),
    }));
    (manager, creates, clears)
}

/// Advance paused tokio time and yield so spawned expiry timers get to run.
async fn advance(d: Duration) {
    tokio::time::advance(d).await;
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
}

fn test_state() -> AppState {
    AppState {
        notifier: Arc::new(StubNotifier),
        clipboard: Arc::new(StubClipboard),
        autostart: Arc::new(StubAutostart),
        power: PowerInhibitManager::new(Arc::new(StubPowerBackend)),
        config: Arc::new(AppConfig::default()),
    }
}

fn state_with_manager(power: Arc<PowerInhibitManager>) -> AppState {
    AppState {
        power,
        ..test_state()
    }
}

fn state_with_config(config: &str) -> AppState {
    AppState {
        config: Arc::new(AppConfig::from_toml_str(config).unwrap()),
        ..test_state()
    }
}

// --- Backend unit tests ---

#[test]
fn stub_backend_create_request_returns_ok_handle() {
    let mut handle = StubPowerBackend.create_request("test").unwrap();
    assert!(handle.clear().is_ok());
}

#[test]
fn stub_handle_clear_is_idempotent() {
    let mut handle = StubPowerBackend.create_request("test").unwrap();
    assert!(handle.clear().is_ok());
    assert!(handle.clear().is_ok());
}

// --- Manager unit tests ---

#[tokio::test(start_paused = true)]
async fn remaining_is_none_before_acquire() {
    let (manager, _, _) = counting_manager();
    assert!(manager.remaining().is_none());
}

#[tokio::test(start_paused = true)]
async fn acquire_creates_one_backend_request_and_reports_remaining() {
    let (manager, creates, _) = counting_manager();
    let ttl = manager.acquire_or_renew(Duration::from_secs(10)).unwrap();
    assert_eq!(ttl, Duration::from_secs(10));
    assert_eq!(creates.load(Ordering::SeqCst), 1);
    assert_eq!(manager.remaining(), Some(Duration::from_secs(10)));
}

#[tokio::test(start_paused = true)]
async fn renew_reuses_existing_handle_without_new_backend_call() {
    let (manager, creates, _) = counting_manager();
    manager.acquire_or_renew(Duration::from_secs(10)).unwrap();
    manager.acquire_or_renew(Duration::from_secs(10)).unwrap();
    assert_eq!(creates.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn ttl_is_clamped_to_max() {
    let (manager, _, _) = counting_manager();
    let ttl = manager
        .acquire_or_renew(Duration::from_secs(MAX_TTL_SECS + 1))
        .unwrap();
    assert_eq!(ttl, Duration::from_secs(MAX_TTL_SECS));
}

#[tokio::test(start_paused = true)]
async fn expires_automatically_after_ttl_without_further_calls() {
    let (manager, _, clears) = counting_manager();
    manager.acquire_or_renew(Duration::from_secs(5)).unwrap();
    advance(Duration::from_secs(6)).await;
    assert_eq!(clears.load(Ordering::SeqCst), 1);
    assert!(manager.remaining().is_none());
}

#[tokio::test(start_paused = true)]
async fn renewal_extends_deadline_and_old_timer_is_noop() {
    let (manager, _, clears) = counting_manager();
    manager.acquire_or_renew(Duration::from_secs(10)).unwrap();
    advance(Duration::from_secs(6)).await;
    manager.acquire_or_renew(Duration::from_secs(10)).unwrap();
    // Past the original deadline (t=10) but before the renewed one (t=16).
    advance(Duration::from_secs(6)).await;
    assert_eq!(clears.load(Ordering::SeqCst), 0);
    assert!(manager.remaining().is_some());
    // Past the renewed deadline: cleared exactly once, not twice.
    advance(Duration::from_secs(5)).await;
    assert_eq!(clears.load(Ordering::SeqCst), 1);
    assert!(manager.remaining().is_none());
}

#[tokio::test(start_paused = true)]
async fn release_clears_immediately_and_pending_timer_is_noop() {
    let (manager, _, clears) = counting_manager();
    manager.acquire_or_renew(Duration::from_secs(10)).unwrap();
    manager.release();
    assert_eq!(clears.load(Ordering::SeqCst), 1);
    assert!(manager.remaining().is_none());
    advance(Duration::from_secs(11)).await;
    assert_eq!(clears.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn release_without_acquire_is_a_noop() {
    let (manager, _, clears) = counting_manager();
    manager.release();
    assert_eq!(clears.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn acquire_after_expiry_creates_a_new_backend_request() {
    let (manager, creates, _) = counting_manager();
    manager.acquire_or_renew(Duration::from_secs(5)).unwrap();
    advance(Duration::from_secs(6)).await;
    manager.acquire_or_renew(Duration::from_secs(5)).unwrap();
    assert_eq!(creates.load(Ordering::SeqCst), 2);
    assert!(manager.remaining().is_some());
}

#[tokio::test(start_paused = true)]
async fn acquire_after_release_creates_a_new_backend_request() {
    let (manager, creates, _) = counting_manager();
    manager.acquire_or_renew(Duration::from_secs(10)).unwrap();
    manager.release();
    manager.acquire_or_renew(Duration::from_secs(10)).unwrap();
    assert_eq!(creates.load(Ordering::SeqCst), 2);
    assert!(manager.remaining().is_some());
}

#[tokio::test(start_paused = true)]
async fn acquire_propagates_backend_failure() {
    let manager = PowerInhibitManager::new(Arc::new(FailingPowerBackend));
    assert!(manager.acquire_or_renew(Duration::from_secs(10)).is_err());
    assert!(manager.remaining().is_none());
}

// --- API tests ---

#[tokio::test]
async fn post_inhibit_returns_200_with_default_ttl() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["ttl_seconds"], DEFAULT_TTL_SECS);
}

#[tokio::test]
async fn post_inhibit_accepts_custom_ttl() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ttl_seconds": 42}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ttl_seconds"], 42);
}

#[tokio::test]
async fn post_inhibit_accepts_minimum_ttl() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ttl_seconds": 1}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ttl_seconds"], 1);
}

#[tokio::test]
async fn post_inhibit_accepts_max_ttl_unclamped() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"ttl_seconds": {MAX_TTL_SECS}}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ttl_seconds"], MAX_TTL_SECS);
}

#[tokio::test]
async fn post_inhibit_defaults_ttl_for_empty_json_object() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ttl_seconds"], DEFAULT_TTL_SECS);
}

#[tokio::test]
async fn post_inhibit_rejects_non_numeric_ttl() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ttl_seconds": "abc"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_inhibit_clamps_oversized_ttl() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ttl_seconds": 999999}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ttl_seconds"], MAX_TTL_SECS);
}

#[tokio::test]
async fn post_inhibit_rejects_zero_ttl() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ttl_seconds": 0}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_inhibit_rejects_invalid_json() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_inhibit_reports_inactive_initially() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["active"], false);
    assert_eq!(json["remaining_seconds"], serde_json::Value::Null);
}

#[tokio::test]
async fn get_inhibit_reports_active_after_post() {
    let app = build_router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ttl_seconds": 60}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["active"], true);
    let remaining = json["remaining_seconds"].as_u64().unwrap();
    assert!(remaining > 0 && remaining <= 60);
}

#[tokio::test]
async fn delete_inhibit_releases_and_get_reports_inactive() {
    let app = build_router(test_state());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["active"], false);
}

#[tokio::test]
async fn delete_inhibit_when_inactive_returns_200() {
    let app = build_router(test_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test(start_paused = true)]
async fn inhibit_expires_after_ttl_end_to_end() {
    let (manager, _, clears) = counting_manager();
    let app = build_router(state_with_manager(manager));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ttl_seconds": 5}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    advance(Duration::from_secs(6)).await;
    assert_eq!(clears.load(Ordering::SeqCst), 1);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["active"], false);
}

#[tokio::test]
async fn power_endpoints_return_403_when_disabled() {
    for method in ["POST", "DELETE", "GET"] {
        let app = build_router(state_with_config(r#"enabled_operations = ["health"]"#));

        let response = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/api/v1/power/inhibit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "method {method}");
    }
}

// A clear failure is internal and the slot is gone either way (dropping the
// handle releases the OS request), so DELETE still reports success.
#[tokio::test]
async fn delete_inhibit_returns_200_even_when_clear_fails() {
    let manager = PowerInhibitManager::new(Arc::new(FailingClearBackend));
    let app = build_router(state_with_manager(manager));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["active"], false);
}

#[tokio::test]
async fn post_inhibit_returns_500_when_backend_fails() {
    let manager = PowerInhibitManager::new(Arc::new(FailingPowerBackend));
    let app = build_router(state_with_manager(manager));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/power/inhibit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "power inhibit failed");
}
