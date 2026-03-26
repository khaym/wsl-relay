use wsl_relay::config::AppConfig;

#[test]
fn default_enables_autostart() {
    let config = AppConfig::default();
    assert!(config.is_operation_enabled("autostart"));
}

#[test]
fn default_port_is_9400() {
    let config = AppConfig::default();
    assert_eq!(config.port, 9400);
}

#[test]
fn default_enables_all_operations() {
    let config = AppConfig::default();
    assert!(config.is_operation_enabled("health"));
    assert!(config.is_operation_enabled("notify"));
    assert!(config.is_operation_enabled("clipboard"));
    assert!(config.is_operation_enabled("screenshot"));
}

#[test]
fn disabled_operation_returns_false() {
    let config = AppConfig::default();
    assert!(!config.is_operation_enabled("unknown"));
}

#[test]
fn from_toml_custom_port() {
    let toml = r#"
        port = 8080
        enabled_operations = ["health", "notify"]
    "#;
    let config = AppConfig::from_toml_str(toml).unwrap();
    assert_eq!(config.port, 8080);
    assert!(config.is_operation_enabled("notify"));
    assert!(!config.is_operation_enabled("clipboard"));
}

#[test]
fn from_toml_defaults_when_omitted() {
    let toml = "";
    let config = AppConfig::from_toml_str(toml).unwrap();
    assert_eq!(config.port, 9400);
    // from_toml_str("") and Default::default() must produce the same operations
    let default_config = AppConfig::default();
    assert_eq!(config.enabled_operations, default_config.enabled_operations);
}

#[test]
fn from_toml_invalid_syntax_returns_error() {
    let toml = "port = [invalid";
    let result = AppConfig::from_toml_str(toml);
    assert!(result.is_err());
}

#[test]
fn empty_operations_disables_everything() {
    let toml = r#"enabled_operations = []"#;
    let config = AppConfig::from_toml_str(toml).unwrap();
    assert!(!config.is_operation_enabled("health"));
    assert!(!config.is_operation_enabled("notify"));
}

#[test]
fn apply_port_override_changes_port() {
    let config = AppConfig::default().apply_port_override(Some("8888"));
    assert_eq!(config.port, 8888);
}

#[test]
fn apply_port_override_ignores_invalid_value() {
    let config = AppConfig::default().apply_port_override(Some("abc"));
    assert_eq!(config.port, 9400);
}

#[test]
fn apply_port_override_noop_when_none() {
    let config = AppConfig::default().apply_port_override(None);
    assert_eq!(config.port, 9400);
}

#[test]
fn apply_port_override_rejects_zero() {
    let config = AppConfig::default().apply_port_override(Some("0"));
    assert_eq!(config.port, 9400);
}

#[test]
fn apply_port_override_empty_string_fallback() {
    let config = AppConfig::default().apply_port_override(Some(""));
    assert_eq!(config.port, 9400);
}

#[test]
fn apply_port_override_max_u16() {
    let config = AppConfig::default().apply_port_override(Some("65535"));
    assert_eq!(config.port, 65535);
}

#[test]
fn apply_port_override_overflow_fallback() {
    let config = AppConfig::default().apply_port_override(Some("65536"));
    assert_eq!(config.port, 9400);
}

#[test]
fn default_config_path_returns_appdata_based_path() {
    let path = AppConfig::default_config_path();
    // On non-Windows (APPDATA not set), returns None
    // On Windows, returns Some(%APPDATA%\wsl-relay\config.toml)
    match std::env::var("APPDATA") {
        Ok(appdata) => {
            let expected = std::path::PathBuf::from(appdata)
                .join("wsl-relay")
                .join("config.toml");
            assert_eq!(path, Some(expected));
        }
        Err(_) => {
            assert_eq!(path, None);
        }
    }
}
