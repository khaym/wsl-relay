use std::collections::HashSet;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_operations")]
    pub enabled_operations: HashSet<String>,
}

fn default_port() -> u16 {
    9400
}

fn default_operations() -> HashSet<String> {
    [
        "health".to_string(),
        "notify".to_string(),
        "clipboard".to_string(),
        "screenshot".to_string(),
        "autostart".to_string(),
    ]
    .into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            enabled_operations: default_operations(),
        }
    }
}

impl AppConfig {
    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        let config: Self = toml::from_str(s)?;
        Ok(config)
    }

    pub fn is_operation_enabled(&self, operation: &str) -> bool {
        self.enabled_operations.contains(operation)
    }

    /// Override port from `WSL_RELAY_PORT` environment variable.
    /// Invalid values are silently ignored (falls back to current port).
    pub fn apply_port_env_override(self) -> Self {
        let env_val = std::env::var("WSL_RELAY_PORT").ok();
        self.apply_port_override(env_val.as_deref())
    }

    /// Override port from an optional string value.
    /// Invalid values are silently ignored (falls back to current port).
    pub fn apply_port_override(mut self, value: Option<&str>) -> Self {
        if let Some(val) = value {
            match val.parse::<u16>() {
                Ok(0) => {
                    tracing::warn!("Port 0 is not valid, ignoring");
                }
                Ok(port) => {
                    self.port = port;
                }
                Err(_) => {
                    tracing::warn!("Invalid port value: {val}, ignoring");
                }
            }
        }
        self
    }

    /// Returns the default config file path: `%APPDATA%\wsl-relay\config.toml`.
    /// Returns `None` if `APPDATA` is not set (e.g., non-Windows).
    pub fn default_config_path() -> Option<PathBuf> {
        std::env::var("APPDATA")
            .ok()
            .map(|appdata| PathBuf::from(appdata).join("wsl-relay").join("config.toml"))
    }
}
