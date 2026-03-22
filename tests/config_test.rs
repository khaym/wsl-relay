use wsl_relay::config::AppConfig;

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
