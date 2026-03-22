use std::collections::HashSet;

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
}
