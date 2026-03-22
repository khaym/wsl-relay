use wsl_relay::notify::{escape_xml, NotificationBackend, NotifyIcon, NotifyRequest, StubNotifier};

#[test]
fn stub_notifier_returns_ok() {
    let notifier = StubNotifier;
    let req = NotifyRequest {
        title: "Test".to_string(),
        body: "Hello".to_string(),
        icon: NotifyIcon::Info,
    };
    assert!(notifier.notify(&req).is_ok());
}

#[test]
fn deserialize_notify_request_all_fields() {
    let json = r#"{"title":"Done","body":"Build finished","icon":"success"}"#;
    let req: NotifyRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.title, "Done");
    assert_eq!(req.body, "Build finished");
    assert_eq!(req.icon, NotifyIcon::Success);
}

#[test]
fn deserialize_notify_request_default_icon() {
    let json = r#"{"title":"Done","body":"Build finished"}"#;
    let req: NotifyRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.icon, NotifyIcon::Info);
}

#[test]
fn deserialize_all_icon_variants() {
    for (s, expected) in [
        ("info", NotifyIcon::Info),
        ("success", NotifyIcon::Success),
        ("warning", NotifyIcon::Warning),
        ("error", NotifyIcon::Error),
    ] {
        let json = format!(r#"{{"title":"T","body":"B","icon":"{}"}}"#, s);
        let req: NotifyRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.icon, expected);
    }
}

#[test]
fn deserialize_invalid_icon_fails() {
    let json = r#"{"title":"T","body":"B","icon":"unknown"}"#;
    let result = serde_json::from_str::<NotifyRequest>(json);
    assert!(result.is_err());
}

#[test]
fn deserialize_missing_title_fails() {
    let json = r#"{"body":"Hello"}"#;
    let result = serde_json::from_str::<NotifyRequest>(json);
    assert!(result.is_err());
}

#[test]
fn deserialize_missing_body_fails() {
    let json = r#"{"title":"Test"}"#;
    let result = serde_json::from_str::<NotifyRequest>(json);
    assert!(result.is_err());
}

#[test]
fn escape_xml_special_characters() {
    assert_eq!(escape_xml("<script>alert('xss')</script>"),
        "&lt;script&gt;alert(&apos;xss&apos;)&lt;/script&gt;");
    assert_eq!(escape_xml("a & b"), "a &amp; b");
    assert_eq!(escape_xml(r#"say "hello""#), "say &quot;hello&quot;");
}

#[test]
fn escape_xml_plain_text_unchanged() {
    assert_eq!(escape_xml("Hello World"), "Hello World");
}
